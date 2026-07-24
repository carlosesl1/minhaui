#include <windows.h>
#include <wbemidl.h>
#include <oleauto.h>

namespace {
constexpr HRESULT kNotSupported = HRESULT_FROM_WIN32(ERROR_NOT_SUPPORTED);

class ComScope {
  public:
    ComScope() : status_(CoInitializeEx(nullptr, COINIT_MULTITHREADED)) {}
    ~ComScope() {
        if (status_ == S_OK || status_ == S_FALSE) {
            CoUninitialize();
        }
    }
    HRESULT status() const {
        return status_ == RPC_E_CHANGED_MODE ? S_OK : status_;
    }

  private:
    HRESULT status_;
};

HRESULT ConnectBrightnessWmi(IWbemServices** services) {
    *services = nullptr;
    IWbemLocator* locator = nullptr;
    HRESULT status = CoCreateInstance(
        CLSID_WbemLocator,
        nullptr,
        CLSCTX_INPROC_SERVER,
        IID_IWbemLocator,
        reinterpret_cast<void**>(&locator));
    if (FAILED(status)) {
        return status;
    }

    BSTR namespace_name = SysAllocString(L"ROOT\\WMI");
    if (namespace_name == nullptr) {
        locator->Release();
        return E_OUTOFMEMORY;
    }
    status = locator->ConnectServer(
        namespace_name,
        nullptr,
        nullptr,
        nullptr,
        0,
        nullptr,
        nullptr,
        services);
    SysFreeString(namespace_name);
    locator->Release();
    if (FAILED(status)) {
        return status;
    }

    status = CoSetProxyBlanket(
        *services,
        RPC_C_AUTHN_WINNT,
        RPC_C_AUTHZ_NONE,
        nullptr,
        RPC_C_AUTHN_LEVEL_CALL,
        RPC_C_IMP_LEVEL_IMPERSONATE,
        nullptr,
        EOAC_NONE);
    if (FAILED(status)) {
        (*services)->Release();
        *services = nullptr;
    }
    return status;
}
HRESULT QueryFirst(IWbemServices* services, const wchar_t* query, IWbemClassObject** object) {
    *object = nullptr;
    BSTR language = SysAllocString(L"WQL");
    BSTR statement = SysAllocString(query);
    if (language == nullptr || statement == nullptr) {
        SysFreeString(language);
        SysFreeString(statement);
        return E_OUTOFMEMORY;
    }
    IEnumWbemClassObject* enumerator = nullptr;
    HRESULT status = services->ExecQuery(
        language,
        statement,
        WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
        nullptr,
        &enumerator);
    SysFreeString(language);
    SysFreeString(statement);
    if (FAILED(status)) {
        return status;
    }
    ULONG returned = 0;
    status = enumerator->Next(WBEM_INFINITE, 1, object, &returned);
    enumerator->Release();
    if (FAILED(status)) {
        return status;
    }
    return returned == 1 && *object != nullptr ? S_OK : kNotSupported;
}

HRESULT ReadBrightness(IWbemServices* services, BYTE* value) {
    IWbemClassObject* monitor = nullptr;
    HRESULT status = QueryFirst(
        services,
        L"SELECT CurrentBrightness FROM WmiMonitorBrightness WHERE Active=TRUE",
        &monitor);
    if (FAILED(status)) {
        return status;
    }
    VARIANT current;
    VariantInit(&current);
    status = monitor->Get(L"CurrentBrightness", 0, &current, nullptr, nullptr);
    monitor->Release();
    if (SUCCEEDED(status)) {
        if (current.vt == VT_UI1) {
            *value = current.bVal;
        } else if (current.vt == VT_I4) {
            *value = static_cast<BYTE>(max(0L, min(100L, current.lVal)));
        } else {
            status = E_UNEXPECTED;
        }
    }
    VariantClear(&current);
    return status;
}

HRESULT SetBrightness(IWbemServices* services, BYTE value) {
    IWbemClassObject* instance = nullptr;
    HRESULT status = QueryFirst(
        services,
        L"SELECT __PATH FROM WmiMonitorBrightnessMethods WHERE Active=TRUE",
        &instance);
    if (FAILED(status)) {
        return status;
    }

    VARIANT path;
    VariantInit(&path);
    status = instance->Get(L"__PATH", 0, &path, nullptr, nullptr);
    instance->Release();
    if (FAILED(status) || path.vt != VT_BSTR || path.bstrVal == nullptr) {
        VariantClear(&path);
        return FAILED(status) ? status : E_UNEXPECTED;
    }

    IWbemClassObject* definition = nullptr;
    status = services->GetObject(
        const_cast<BSTR>(L"WmiMonitorBrightnessMethods"),
        0,
        nullptr,
        &definition,
        nullptr);
    if (FAILED(status)) {
        VariantClear(&path);
        return status;
    }
    IWbemClassObject* input_definition = nullptr;
    status = definition->GetMethod(L"WmiSetBrightness", 0, &input_definition, nullptr);
    definition->Release();
    if (FAILED(status)) {
        VariantClear(&path);
        return status;
    }
    IWbemClassObject* input = nullptr;
    status = input_definition->SpawnInstance(0, &input);
    input_definition->Release();
    if (FAILED(status)) {
        VariantClear(&path);
        return status;
    }

    VARIANT timeout;
    VariantInit(&timeout);
    timeout.vt = VT_UI4;
    timeout.ulVal = 0;
    status = input->Put(L"Timeout", 0, &timeout, 0);
    if (SUCCEEDED(status)) {
        VARIANT brightness;
        VariantInit(&brightness);
        brightness.vt = VT_UI1;
        brightness.bVal = value;
        status = input->Put(L"Brightness", 0, &brightness, 0);
    }

    IWbemClassObject* output = nullptr;
    if (SUCCEEDED(status)) {
        status = services->ExecMethod(
            path.bstrVal,
            const_cast<BSTR>(L"WmiSetBrightness"),
            0,
            nullptr,
            input,
            &output,
            nullptr);
    }
    input->Release();
    VariantClear(&path);
    if (FAILED(status)) {
        if (output != nullptr) {
            output->Release();
        }
        return status;
    }

    VARIANT returned;
    VariantInit(&returned);
    status = output->Get(L"ReturnValue", 0, &returned, nullptr, nullptr);
    output->Release();
    if (SUCCEEDED(status) && returned.vt == VT_UI4 && returned.ulVal != 0) {
        status = HRESULT_FROM_WIN32(returned.ulVal);
    }
    VariantClear(&returned);
    return status;
}
}  // namespace

extern "C" HRESULT minhaui_wmi_brightness_read(BYTE* value) {
    if (value == nullptr) {
        return E_POINTER;
    }
    ComScope com;
    if (FAILED(com.status())) {
        return com.status();
    }
    IWbemServices* services = nullptr;
    HRESULT status = ConnectBrightnessWmi(&services);
    if (SUCCEEDED(status)) {
        status = ReadBrightness(services, value);
        services->Release();
    }
    return status;
}

extern "C" HRESULT minhaui_wmi_brightness_set(BYTE value) {
    if (value > 100) {
        return E_INVALIDARG;
    }
    ComScope com;
    if (FAILED(com.status())) {
        return com.status();
    }
    IWbemServices* services = nullptr;
    HRESULT status = ConnectBrightnessWmi(&services);
    if (SUCCEEDED(status)) {
        status = SetBrightness(services, value);
        services->Release();
    }
    return status;
}
