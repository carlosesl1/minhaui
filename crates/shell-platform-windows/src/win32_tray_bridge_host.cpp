#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <cwchar>
#include <iterator>

namespace {

constexpr std::uint32_t kMagic = 0x4742544f;
constexpr std::uint16_t kProtocolVersion = 2;
constexpr std::uint32_t kRecordCapacity = 128;
constexpr std::size_t kTooltipUnits = 128;
constexpr wchar_t kMappingName[] =
    L"Local\\ObsidianGlass.TrayBridge.Snapshot.v2";
constexpr wchar_t kMutexName[] =
    L"Local\\ObsidianGlass.TrayBridge.Host.v2";
constexpr int kCallWndProc = 4;

struct alignas(8) TrayBridgeRecord {
  volatile LONG64 committed_sequence;
  std::uint32_t active;
  std::uint32_t flags;
  std::uint64_t owner_window;
  std::uint32_t icon_id;
  std::uint32_t callback_message;
  std::uint32_t version;
  std::uint32_t has_guid;
  std::uint8_t guid[16];
  wchar_t tooltip[kTooltipUnits];
};

struct alignas(8) TrayBridgeSharedState {
  std::uint32_t magic;
  std::uint16_t protocol_version;
  std::uint16_t record_size;
  std::uint32_t capacity;
  std::uint32_t reserved;
  volatile LONG64 snapshot_sequence;
  std::uint32_t host_process_id;
  volatile LONG explorer_thread_id;
  TrayBridgeRecord records[kRecordCapacity];
};

static_assert(sizeof(TrayBridgeRecord) == 312);
static_assert(offsetof(TrayBridgeSharedState, records) == 32);

class Handle {
 public:
  explicit Handle(HANDLE value = nullptr) noexcept : value_(value) {}
  ~Handle() {
    if (value_ != nullptr && value_ != INVALID_HANDLE_VALUE) {
      CloseHandle(value_);
    }
  }
  Handle(const Handle&) = delete;
  Handle& operator=(const Handle&) = delete;
  HANDLE get() const noexcept { return value_; }
  explicit operator bool() const noexcept {
    return value_ != nullptr && value_ != INVALID_HANDLE_VALUE;
  }

 private:
  HANDLE value_;
};

class Hook {
 public:
  Hook() noexcept = default;
  explicit Hook(HHOOK value) noexcept : value_(value) {}
  ~Hook() {
    if (value_ != nullptr) {
      UnhookWindowsHookEx(value_);
    }
  }
  Hook(const Hook&) = delete;
  Hook& operator=(const Hook&) = delete;
  bool valid() const noexcept { return value_ != nullptr; }

 private:
  HHOOK value_ = nullptr;
};

void PruneSnapshot(TrayBridgeSharedState& state,
                   DWORD explorer_thread_id) noexcept {
  for (auto& record : state.records) {
    const auto owner = reinterpret_cast<HWND>(
        static_cast<std::uintptr_t>(record.owner_window));
    if (record.active != 0 && owner != nullptr && IsWindow(owner) != FALSE) {
      continue;
    }
    InterlockedExchange64(&record.committed_sequence, 0);
    record.active = 0;
  }
  InterlockedExchange(&state.explorer_thread_id,
                      static_cast<LONG>(explorer_thread_id));
}

bool BridgeLibraryPath(wchar_t* path, std::size_t capacity) noexcept {
  const DWORD copied =
      GetModuleFileNameW(nullptr, path, static_cast<DWORD>(capacity));
  if (copied == 0 || copied >= capacity) {
    return false;
  }
  wchar_t* separator = std::wcsrchr(path, L'\\');
  if (separator == nullptr) {
    return false;
  }
  *(separator + 1) = L'\0';
  return wcscat_s(path, capacity, L"obsidian_tray_bridge.dll") == 0;
}

int RunHost() noexcept {
  Handle singleton(CreateMutexW(nullptr, TRUE, kMutexName));
  if (!singleton || GetLastError() == ERROR_ALREADY_EXISTS) {
    return 0;
  }

  Handle mapping(CreateFileMappingW(
      INVALID_HANDLE_VALUE, nullptr, PAGE_READWRITE, 0,
      static_cast<DWORD>(sizeof(TrayBridgeSharedState)), kMappingName));
  if (!mapping) {
    return 1;
  }
  const bool mapping_existed = GetLastError() == ERROR_ALREADY_EXISTS;
  auto* state = static_cast<TrayBridgeSharedState*>(
      MapViewOfFile(mapping.get(), FILE_MAP_READ | FILE_MAP_WRITE, 0, 0,
                    sizeof(TrayBridgeSharedState)));
  if (state == nullptr) {
    return 2;
  }
  const bool compatible_snapshot =
      mapping_existed && state->magic == kMagic &&
      state->protocol_version == kProtocolVersion &&
      state->record_size == sizeof(TrayBridgeRecord) &&
      state->capacity == kRecordCapacity;
  if (!compatible_snapshot) {
    std::memset(state, 0, sizeof(*state));
    state->magic = kMagic;
    state->protocol_version = kProtocolVersion;
    state->record_size = sizeof(TrayBridgeRecord);
    state->capacity = kRecordCapacity;
  } else {
    PruneSnapshot(*state, 0);
  }
  state->host_process_id = GetCurrentProcessId();

  wchar_t library_path[MAX_PATH] = {};
  if (!BridgeLibraryPath(library_path, std::size(library_path))) {
    UnmapViewOfFile(state);
    return 3;
  }
  HMODULE module = LoadLibraryW(library_path);
  if (module == nullptr) {
    UnmapViewOfFile(state);
    return 4;
  }
  const auto hook_proc = reinterpret_cast<HOOKPROC>(
      GetProcAddress(module, "ObsidianTrayBridgeCallWndProc"));
  if (hook_proc == nullptr) {
    FreeLibrary(module);
    UnmapViewOfFile(state);
    return 5;
  }

  for (;;) {
    HWND shell = FindWindowW(L"Shell_TrayWnd", nullptr);
    if (shell == nullptr) {
      Sleep(250);
      continue;
    }
    DWORD explorer_process_id = 0;
    const DWORD explorer_thread_id =
        GetWindowThreadProcessId(shell, &explorer_process_id);
    if (explorer_thread_id == 0 || explorer_process_id == 0) {
      Sleep(250);
      continue;
    }

    PruneSnapshot(*state, explorer_thread_id);
    Hook hook(SetWindowsHookExW(kCallWndProc, hook_proc, module,
                               explorer_thread_id));
    if (!hook.valid()) {
      Sleep(500);
      continue;
    }
    SendMessageTimeoutW(shell, WM_NULL, 0, 0, SMTO_ABORTIFHUNG, 250, nullptr);

    Handle explorer(
        OpenProcess(SYNCHRONIZE, FALSE, explorer_process_id));
    if (explorer) {
      WaitForSingleObject(explorer.get(), INFINITE);
    } else {
      while (IsWindow(shell) != FALSE) {
        Sleep(1000);
      }
    }
    PruneSnapshot(*state, 0);
  }
}

}  // namespace

int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR, int) {
  return RunHost();
}
