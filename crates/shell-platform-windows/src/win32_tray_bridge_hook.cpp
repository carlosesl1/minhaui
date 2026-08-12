#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <shellapi.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <cwchar>

namespace {

constexpr std::uint32_t kMagic = 0x4742544f;
constexpr std::uint16_t kProtocolVersion = 2;
constexpr std::uint32_t kRecordCapacity = 128;
constexpr std::size_t kTooltipUnits = 128;
constexpr std::uint32_t kTrayNotifySignature = 0x34753423;
constexpr wchar_t kMappingName[] =
    L"Local\\ObsidianGlass.TrayBridge.Snapshot.v2";

#pragma pack(push, 1)
struct TrayNotifyIconWire {
  std::uint32_t size;
  std::uint32_t owner_window;
  std::uint32_t icon_id;
  std::uint32_t flags;
  std::uint32_t callback_message;
  std::uint32_t icon_handle;
  wchar_t tooltip[128];
  std::uint32_t state;
  std::uint32_t state_mask;
  wchar_t info[256];
  std::uint32_t version;
  wchar_t info_title[64];
  std::uint32_t info_flags;
  std::uint8_t guid[16];
};
#pragma pack(pop)

static_assert(sizeof(TrayNotifyIconWire) == 952);

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

bool IsShellTrayWindow(HWND window) noexcept {
  wchar_t class_name[32] = {};
  const int copied =
      GetClassNameW(window, class_name, static_cast<int>(std::size(class_name)));
  return copied > 0 && std::wcscmp(class_name, L"Shell_TrayWnd") == 0;
}

bool CopyNotifyIconData(const COPYDATASTRUCT& copy_data,
                        DWORD& action,
                        NOTIFYICONDATAW& icon) noexcept {
  if (copy_data.dwData != 1 || copy_data.lpData == nullptr ||
      copy_data.cbData < sizeof(DWORD) * 3) {
    return false;
  }

  const auto* bytes = static_cast<const std::uint8_t*>(copy_data.lpData);
  std::uint32_t signature = 0;
  std::memcpy(&signature, bytes, sizeof(signature));
  if (signature != kTrayNotifySignature && signature != 1) {
    return false;
  }
  std::memcpy(&action, bytes + sizeof(DWORD), sizeof(action));
  if (action != NIM_ADD && action != NIM_MODIFY && action != NIM_DELETE &&
      action != NIM_SETVERSION) {
    return false;
  }

  const std::size_t available = copy_data.cbData - sizeof(DWORD) * 2;
  if (available < offsetof(TrayNotifyIconWire, callback_message) +
                      sizeof(TrayNotifyIconWire::callback_message)) {
    return false;
  }

  TrayNotifyIconWire wire = {};
  std::memcpy(&wire, bytes + sizeof(DWORD) * 2,
              std::min(available, sizeof(wire)));
  if (wire.owner_window == 0) {
    return false;
  }
  std::memset(&icon, 0, sizeof(icon));
  icon.cbSize = sizeof(icon);
  icon.hWnd =
      reinterpret_cast<HWND>(static_cast<std::uintptr_t>(wire.owner_window));
  icon.uID = wire.icon_id;
  icon.uFlags = wire.flags;
  icon.uCallbackMessage = wire.callback_message;
  std::memcpy(icon.szTip, wire.tooltip, sizeof(wire.tooltip));
  icon.dwState = wire.state;
  icon.dwStateMask = wire.state_mask;
  icon.uVersion = wire.version;
  std::memcpy(&icon.guidItem, wire.guid, sizeof(wire.guid));
  return true;
}

bool SameIdentity(const TrayBridgeRecord& record,
                  const NOTIFYICONDATAW& icon) noexcept {
  const auto owner =
      static_cast<std::uint64_t>(reinterpret_cast<std::uintptr_t>(icon.hWnd));
  if (record.active == 0 || record.owner_window != owner) {
    return false;
  }
  const bool icon_has_guid = (icon.uFlags & NIF_GUID) != 0;
  if (record.has_guid != 0 && icon_has_guid) {
    return std::memcmp(record.guid, &icon.guidItem, sizeof(record.guid)) == 0;
  }
  return record.has_guid == 0 && !icon_has_guid && record.icon_id == icon.uID;
}

TrayBridgeRecord* FindRecord(TrayBridgeSharedState& state,
                             const NOTIFYICONDATAW& icon) noexcept {
  for (auto& record : state.records) {
    if (SameIdentity(record, icon)) {
      return &record;
    }
  }
  return nullptr;
}

TrayBridgeRecord* FindFreeRecord(TrayBridgeSharedState& state) noexcept {
  for (auto& record : state.records) {
    const auto owner = reinterpret_cast<HWND>(
        static_cast<std::uintptr_t>(record.owner_window));
    if (record.active == 0 || owner == nullptr || IsWindow(owner) == FALSE) {
      record.active = 0;
      return &record;
    }
  }
  return nullptr;
}

void CommitRecord(TrayBridgeSharedState& state,
                  TrayBridgeRecord& destination,
                  const NOTIFYICONDATAW& icon,
                  DWORD action) noexcept {
  const LONG64 sequence = InterlockedIncrement64(&state.snapshot_sequence);
  InterlockedExchange64(&destination.committed_sequence, 0);
  if (action == NIM_DELETE) {
    destination.active = 0;
  } else {
    destination.active = 1;
    destination.owner_window = static_cast<std::uint64_t>(
        reinterpret_cast<std::uintptr_t>(icon.hWnd));
    destination.icon_id = icon.uID;
    destination.flags = icon.uFlags;
    if ((icon.uFlags & NIF_MESSAGE) != 0) {
      destination.callback_message = icon.uCallbackMessage;
    }
    if (action == NIM_SETVERSION) {
      destination.version = icon.uVersion;
    }
    if ((icon.uFlags & NIF_GUID) != 0) {
      destination.has_guid = 1;
      std::memcpy(destination.guid, &icon.guidItem, sizeof(destination.guid));
    }
    if ((icon.uFlags & NIF_TIP) != 0) {
      std::wmemset(destination.tooltip, L'\0', kTooltipUnits);
      wcsncpy_s(destination.tooltip, kTooltipUnits, icon.szTip, _TRUNCATE);
    }
  }
  MemoryBarrier();
  InterlockedExchange64(&destination.committed_sequence, sequence);
}

void PublishRecord(const NOTIFYICONDATAW& icon, DWORD action) noexcept {
  HANDLE mapping = OpenFileMappingW(FILE_MAP_WRITE, FALSE, kMappingName);
  if (mapping == nullptr) {
    return;
  }
  auto* state = static_cast<TrayBridgeSharedState*>(
      MapViewOfFile(mapping, FILE_MAP_WRITE, 0, 0, sizeof(TrayBridgeSharedState)));
  if (state == nullptr) {
    CloseHandle(mapping);
    return;
  }

  if (state->magic == kMagic &&
      state->protocol_version == kProtocolVersion &&
      state->record_size == sizeof(TrayBridgeRecord) &&
      state->capacity == kRecordCapacity) {
    TrayBridgeRecord* destination = FindRecord(*state, icon);
    if (destination != nullptr) {
      CommitRecord(*state, *destination, icon, action);
    } else if (action != NIM_DELETE) {
      destination = FindFreeRecord(*state);
      if (destination != nullptr) {
        std::memset(destination, 0, sizeof(*destination));
        CommitRecord(*state, *destination, icon, action);
      }
    }
  }

  UnmapViewOfFile(state);
  CloseHandle(mapping);
}

}  // namespace

extern "C" __declspec(dllexport) LRESULT CALLBACK
ObsidianTrayBridgeCallWndProc(int code, WPARAM wparam, LPARAM lparam) noexcept {
  if (code >= 0 && lparam != 0) {
    const auto* message = reinterpret_cast<const CWPSTRUCT*>(lparam);
    if (message->message == WM_COPYDATA && IsShellTrayWindow(message->hwnd) &&
        message->lParam != 0) {
      DWORD action = 0;
      NOTIFYICONDATAW icon = {};
      const auto* copy_data =
          reinterpret_cast<const COPYDATASTRUCT*>(message->lParam);
      if (CopyNotifyIconData(*copy_data, action, icon)) {
        PublishRecord(icon, action);
      }
    }
  }
  return CallNextHookEx(nullptr, code, wparam, lparam);
}

BOOL WINAPI DllMain(HINSTANCE, DWORD, LPVOID) {
  return TRUE;
}
