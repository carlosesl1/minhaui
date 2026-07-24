#include <windows.h>
#include <inspectable.h>
#include <roapi.h>
#include <winstring.h>

enum QuietHoursMode {
    QuietHoursInvalid = 0,
    QuietHoursOff = 1,
    QuietHoursPriorityOnly = 2,
    QuietHoursAlarmsOnly = 3,
};

struct __declspec(uuid("E4D5EA8C-6004-5AA5-9F75-D885E8A0E5DB"))
    IQuietHoursSettingsInterop : IInspectable {
    virtual HRESULT STDMETHODCALLTYPE get_Mode(QuietHoursMode* value) = 0;
    virtual HRESULT STDMETHODCALLTYPE put_Mode(QuietHoursMode value) = 0;
    virtual HRESULT STDMETHODCALLTYPE get_FocusQuietMomentApplicable(boolean* value) = 0;
    virtual HRESULT STDMETHODCALLTYPE put_FocusQuietMomentApplicable(boolean value) = 0;
};

struct __declspec(uuid("E2A96B88-B134-5918-9AF1-F455816D539F"))
    IQuietHoursSettingsInteropStatics : IInspectable {
    virtual HRESULT STDMETHODCALLTYPE GetDefault(IQuietHoursSettingsInterop** value) = 0;
};

namespace {
constexpr wchar_t kRuntimeClass[] =
    L"Windows.Internal.UI.Notifications.QuietHoursSettingsInterop";

HRESULT GetSettings(IQuietHoursSettingsInterop** result) {
    *result = nullptr;
    HSTRING runtime_class = nullptr;
    HRESULT status = WindowsCreateString(
        kRuntimeClass,
        static_cast<UINT32>(wcslen(kRuntimeClass)),
        &runtime_class);
    if (FAILED(status)) {
        return status;
    }

    IQuietHoursSettingsInteropStatics* statics = nullptr;
    status = RoGetActivationFactory(
        runtime_class,
        __uuidof(IQuietHoursSettingsInteropStatics),
        reinterpret_cast<void**>(&statics));
    WindowsDeleteString(runtime_class);
    if (FAILED(status)) {
        return status;
    }

    status = statics->GetDefault(result);
    statics->Release();
    return status;
}
}  // namespace

extern "C" HRESULT minhaui_quiet_hours_set_mode(int mode) {
    if (mode < QuietHoursOff || mode > QuietHoursAlarmsOnly) {
        return E_INVALIDARG;
    }
    IQuietHoursSettingsInterop* settings = nullptr;
    HRESULT status = GetSettings(&settings);
    if (FAILED(status)) {
        return status;
    }

    status = settings->put_Mode(static_cast<QuietHoursMode>(mode));
    settings->Release();
    return status;
}
