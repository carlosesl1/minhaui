//! Server-side UI Automation provider for the native Settings HWND.
//!
//! Providers retain only a thread-safe immutable renderer snapshot. Pattern
//! calls may arrive on COM/RPC threads, so they never touch `SettingsController`
//! directly: they enqueue a typed `PlatformEvent` and wake the HWND owner.
//!
//! Consecutive frames are diffed only when they are published by the HWND owner
//! thread. When an automation client is listening, the provider raises
//! structure, property, and focus events after atomically installing the new
//! snapshot. Event delivery never polls and never calls the controller.

#![allow(
    non_snake_case,
    reason = "local COM ABI interfaces preserve Windows UI Automation method names"
)]

use core::ffi::c_void;
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use shell_renderer::{
    Dpi, SettingsAccessibilityControlType, SettingsAccessibilityNodeId,
    SettingsAccessibilityPattern, SettingsAccessibilitySnapshot, SettingsFocus, SettingsHit,
    SettingsLayout, SettingsScene, SettingsSectionId, settings_accessibility_snapshot,
};
use windows::Win32::Foundation::{E_POINTER, E_UNEXPECTED, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayDestroy, SafeArrayPutElement};
use windows::Win32::System::Variant::{
    VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_ARRAY, VT_I4, VT_R8,
};
use windows::Win32::UI::Accessibility::{
    IInvokeProvider, IInvokeProvider_Impl, IRangeValueProvider, IRangeValueProvider_Impl,
    IRawElementProviderFragment, IRawElementProviderFragmentRoot, IRawElementProviderSimple,
    IToggleProvider, IToggleProvider_Impl, NavigateDirection, NavigateDirection_FirstChild,
    NavigateDirection_LastChild, NavigateDirection_NextSibling, NavigateDirection_Parent,
    NavigateDirection_PreviousSibling, ProviderOptions, ProviderOptions_ProviderOwnsSetFocus,
    ProviderOptions_ServerSideProvider, ProviderOptions_UseComThreading,
    StructureChangeType_ChildrenInvalidated, ToggleState, ToggleState_Off, ToggleState_On,
    UIA_AutomationFocusChangedEventId, UIA_BoundingRectanglePropertyId, UIA_ButtonControlTypeId,
    UIA_CheckBoxControlTypeId, UIA_ComboBoxControlTypeId, UIA_ControlTypePropertyId,
    UIA_E_ELEMENTNOTAVAILABLE, UIA_E_ELEMENTNOTENABLED, UIA_E_INVALIDOPERATION,
    UIA_EditControlTypeId, UIA_GroupControlTypeId, UIA_HasKeyboardFocusPropertyId,
    UIA_HelpTextPropertyId, UIA_InvokePatternId, UIA_IsEnabledPropertyId,
    UIA_IsInvokePatternAvailablePropertyId, UIA_IsOffscreenPropertyId,
    UIA_IsRangeValuePatternAvailablePropertyId, UIA_IsTogglePatternAvailablePropertyId,
    UIA_ListControlTypeId, UIA_ListItemControlTypeId, UIA_NamePropertyId, UIA_PATTERN_ID,
    UIA_PROPERTY_ID, UIA_RangeValueIsReadOnlyPropertyId, UIA_RangeValueLargeChangePropertyId,
    UIA_RangeValueMaximumPropertyId, UIA_RangeValueMinimumPropertyId, UIA_RangeValuePatternId,
    UIA_RangeValueSmallChangePropertyId, UIA_RangeValueValuePropertyId, UIA_SliderControlTypeId,
    UIA_TextControlTypeId, UIA_TogglePatternId, UIA_ToggleToggleStatePropertyId,
    UIA_ValueValuePropertyId, UIA_WindowControlTypeId, UiaAppendRuntimeId, UiaClientsAreListening,
    UiaHostProviderFromHwnd, UiaRaiseAutomationEvent, UiaRaiseAutomationPropertyChangedEvent,
    UiaRaiseStructureChangedEvent, UiaRect, UiaReturnRawElementProvider, UiaRootObjectId,
};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use windows::core::{
    BOOL, Error, HRESULT, IUnknown, IUnknown_Vtbl, Interface, Result, implement, interface,
};

use crate::win32_event_queue::{RoutedPlatformEvent, queue_event_with_wake};
use crate::{PlatformEvent, SettingsAutomationAction};

/// Pointer-free wakeup used after an action has entered the bounded event queue.
pub(super) const SETTINGS_UIA_WAKE_MESSAGE: u32 = WM_APP + 0x69;

static PROVIDERS: OnceLock<Mutex<HashMap<isize, Arc<ProviderState>>>> = OnceLock::new();

// The windows-rs 0.62 high-level provider traits cannot express the successful
// null interface mandated by UIA for missing relatives/patterns. These local
// ABI-equivalent interfaces keep null out-parameters explicit and initialized.
#[interface("d6dd68d1-86fd-4332-8666-9abedea2d24c")]
unsafe trait ISettingsRawElementProviderSimple: IUnknown {
    fn ProviderOptions(&self, result: *mut ProviderOptions) -> HRESULT;
    fn GetPatternProvider(&self, pattern: UIA_PATTERN_ID, result: *mut *mut c_void) -> HRESULT;
    fn GetPropertyValue(&self, property: UIA_PROPERTY_ID, result: *mut VARIANT) -> HRESULT;
    fn HostRawElementProvider(&self, result: *mut *mut c_void) -> HRESULT;
}

#[interface("f7063da8-8359-439c-9297-bbc5299a7d87")]
unsafe trait ISettingsRawElementProviderFragment: IUnknown {
    fn Navigate(&self, direction: NavigateDirection, result: *mut *mut c_void) -> HRESULT;
    fn GetRuntimeId(&self, result: *mut *mut SAFEARRAY) -> HRESULT;
    fn BoundingRectangle(&self, result: *mut UiaRect) -> HRESULT;
    fn GetEmbeddedFragmentRoots(&self, result: *mut *mut SAFEARRAY) -> HRESULT;
    fn SetFocus(&self) -> HRESULT;
    fn FragmentRoot(&self, result: *mut *mut c_void) -> HRESULT;
}

#[interface("620ce2a5-ab8f-40a9-86cb-de3c75599b58")]
unsafe trait ISettingsRawElementProviderFragmentRoot: IUnknown {
    fn ElementProviderFromPoint(&self, x: f64, y: f64, result: *mut *mut c_void) -> HRESULT;
    fn GetFocus(&self, result: *mut *mut c_void) -> HRESULT;
}

struct ProviderState {
    hwnd: isize,
    active: AtomicBool,
    snapshot: Mutex<Option<Arc<NativeSnapshot>>>,
}

impl ProviderState {
    fn new(hwnd: HWND) -> Self {
        Self {
            hwnd: hwnd.0 as isize,
            active: AtomicBool::new(true),
            snapshot: Mutex::new(None),
        }
    }

    fn hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut c_void)
    }

    fn current(&self) -> Result<Arc<NativeSnapshot>> {
        if !self.active.load(Ordering::Acquire) {
            return element_not_available();
        }
        lock_recover(&self.snapshot)
            .clone()
            .ok_or_else(element_not_available_error)
    }

    fn replace(
        &self,
        snapshot: NativeSnapshot,
    ) -> (Option<Arc<NativeSnapshot>>, Arc<NativeSnapshot>) {
        let snapshot = Arc::new(snapshot);
        let previous = lock_recover(&self.snapshot).replace(snapshot.clone());
        (previous, snapshot)
    }

    fn queue(&self, action: SettingsAutomationAction) -> Result<()> {
        if !self.active.load(Ordering::Acquire) {
            return element_not_available();
        }
        let hwnd = self.hwnd();
        let queued = queue_event_with_wake(
            RoutedPlatformEvent::window(hwnd, PlatformEvent::SettingsAutomation(action)),
            || {
                if !self.active.load(Ordering::Acquire) {
                    return false;
                }
                // SAFETY: The message is pointer-free and targets the same
                // registered Settings HWND whose state was checked immediately
                // above. A failed post rolls back the just-enqueued action.
                unsafe {
                    PostMessageW(Some(hwnd), SETTINGS_UIA_WAKE_MESSAGE, WPARAM(0), LPARAM(0))
                        .is_ok()
                }
            },
        );
        if queued {
            Ok(())
        } else {
            Err(Error::from_thread())
        }
    }
}

#[derive(Clone)]
struct NativeNode {
    id: SettingsAccessibilityNodeId,
    parent: Option<SettingsAccessibilityNodeId>,
    previous_sibling: Option<SettingsAccessibilityNodeId>,
    next_sibling: Option<SettingsAccessibilityNodeId>,
    name: String,
    help_text: Option<String>,
    value: Option<String>,
    enabled: bool,
    focused: bool,
    offscreen: bool,
    control_type: SettingsAccessibilityControlType,
    patterns: Box<[SettingsAccessibilityPattern]>,
    bounds: Option<UiaRect>,
}

#[derive(Clone)]
struct NativeSnapshot {
    nodes: Box<[NativeNode]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChangedProperty {
    Name,
    HelpText,
    Value,
    Enabled,
    Focused,
    Offscreen,
    ControlType,
    BoundingRectangle,
    InvokeAvailable,
    ToggleAvailable,
    RangeValueAvailable,
    ToggleState,
    RangeValue,
    RangeMinimum,
    RangeMaximum,
    RangeSmallChange,
    RangeLargeChange,
    RangeReadOnly,
}

impl ChangedProperty {
    const fn uia_id(self) -> UIA_PROPERTY_ID {
        match self {
            Self::Name => UIA_NamePropertyId,
            Self::HelpText => UIA_HelpTextPropertyId,
            Self::Value => UIA_ValueValuePropertyId,
            Self::Enabled => UIA_IsEnabledPropertyId,
            Self::Focused => UIA_HasKeyboardFocusPropertyId,
            Self::Offscreen => UIA_IsOffscreenPropertyId,
            Self::ControlType => UIA_ControlTypePropertyId,
            Self::BoundingRectangle => UIA_BoundingRectanglePropertyId,
            Self::InvokeAvailable => UIA_IsInvokePatternAvailablePropertyId,
            Self::ToggleAvailable => UIA_IsTogglePatternAvailablePropertyId,
            Self::RangeValueAvailable => UIA_IsRangeValuePatternAvailablePropertyId,
            Self::ToggleState => UIA_ToggleToggleStatePropertyId,
            Self::RangeValue => UIA_RangeValueValuePropertyId,
            Self::RangeMinimum => UIA_RangeValueMinimumPropertyId,
            Self::RangeMaximum => UIA_RangeValueMaximumPropertyId,
            Self::RangeSmallChange => UIA_RangeValueSmallChangePropertyId,
            Self::RangeLargeChange => UIA_RangeValueLargeChangePropertyId,
            Self::RangeReadOnly => UIA_RangeValueIsReadOnlyPropertyId,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum PropertyValue {
    Text(String),
    Bool(bool),
    Integer(i32),
    Number(f64),
    Rectangle(UiaRect),
}

impl PropertyValue {
    fn into_variant(self) -> Result<VARIANT> {
        match self {
            Self::Text(value) => Ok(VARIANT::from(value.as_str())),
            Self::Bool(value) => Ok(VARIANT::from(value)),
            Self::Integer(value) => Ok(VARIANT::from(value)),
            Self::Number(value) => Ok(VARIANT::from(value)),
            Self::Rectangle(value) => rectangle_variant(value),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PropertyChange {
    node: SettingsAccessibilityNodeId,
    property: ChangedProperty,
    old_value: PropertyValue,
    new_value: PropertyValue,
}

#[derive(Debug, Default, PartialEq)]
struct SnapshotDiff {
    invalidated_parents: Vec<SettingsAccessibilityNodeId>,
    properties: Vec<PropertyChange>,
    focused: Option<SettingsAccessibilityNodeId>,
}

impl NativeSnapshot {
    fn from_renderer(snapshot: &SettingsAccessibilitySnapshot, origin: POINT, dpi: Dpi) -> Self {
        let nodes = snapshot
            .nodes()
            .iter()
            .map(|node| NativeNode {
                id: node.id(),
                parent: node.parent(),
                previous_sibling: node.previous_sibling(),
                next_sibling: node.next_sibling(),
                name: node.name().to_owned(),
                help_text: node.help_text().map(str::to_owned),
                value: node.value().map(str::to_owned),
                enabled: node.enabled(),
                focused: node.focused(),
                offscreen: node.offscreen(),
                control_type: node.control_type(),
                patterns: node.patterns().into(),
                bounds: node
                    .bounds_dip()
                    .map(|bounds| screen_rect(bounds, origin, dpi)),
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self { nodes }
    }

    fn node(&self, id: SettingsAccessibilityNodeId) -> Option<&NativeNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    fn navigate(
        &self,
        id: SettingsAccessibilityNodeId,
        direction: NavigateDirection,
    ) -> Option<SettingsAccessibilityNodeId> {
        let node = self.node(id)?;
        if direction == NavigateDirection_Parent {
            node.parent
        } else if direction == NavigateDirection_NextSibling {
            node.next_sibling
        } else if direction == NavigateDirection_PreviousSibling {
            node.previous_sibling
        } else if direction == NavigateDirection_FirstChild {
            self.nodes
                .iter()
                .find(|candidate| candidate.parent == Some(id))
                .map(|candidate| candidate.id)
        } else if direction == NavigateDirection_LastChild {
            self.nodes
                .iter()
                .rev()
                .find(|candidate| candidate.parent == Some(id))
                .map(|candidate| candidate.id)
        } else {
            None
        }
    }

    fn element_from_point(&self, x: f64, y: f64) -> Option<SettingsAccessibilityNodeId> {
        self.nodes
            .iter()
            .rev()
            .find(|node| node.bounds.is_some_and(|bounds| contains(bounds, x, y)))
            .map(|node| node.id)
    }

    fn focused(&self) -> Option<SettingsAccessibilityNodeId> {
        self.nodes
            .iter()
            .find(|node| node.focused)
            .map(|node| node.id)
    }
}

fn diff_snapshots(previous: &NativeSnapshot, current: &NativeSnapshot) -> SnapshotDiff {
    let previous_children = children_by_parent(previous);
    let current_children = children_by_parent(current);
    let invalidated_parents = current
        .nodes
        .iter()
        .filter(|node| previous.node(node.id).is_some())
        .filter_map(|node| {
            let previous = previous_children
                .get(&node.id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let current = current_children
                .get(&node.id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            (previous != current).then_some(node.id)
        })
        .collect();

    let mut properties = Vec::new();
    for current_node in &current.nodes {
        let Some(previous_node) = previous.node(current_node.id) else {
            continue;
        };
        diff_node_properties(previous_node, current_node, &mut properties);
    }

    let focused = (previous.focused() != current.focused())
        .then(|| current.focused())
        .flatten();
    SnapshotDiff {
        invalidated_parents,
        properties,
        focused,
    }
}

fn children_by_parent(
    snapshot: &NativeSnapshot,
) -> HashMap<SettingsAccessibilityNodeId, Vec<SettingsAccessibilityNodeId>> {
    let mut children = HashMap::<_, Vec<_>>::new();
    for node in &snapshot.nodes {
        if let Some(parent) = node.parent {
            children.entry(parent).or_default().push(node.id);
        }
    }
    children
}

fn diff_node_properties(
    previous: &NativeNode,
    current: &NativeNode,
    changes: &mut Vec<PropertyChange>,
) {
    let node = current.id;
    push_text_property_change(
        changes,
        node,
        ChangedProperty::Name,
        &previous.name,
        &current.name,
    );
    push_text_property_change(
        changes,
        node,
        ChangedProperty::HelpText,
        previous.help_text.as_deref().unwrap_or_default(),
        current.help_text.as_deref().unwrap_or_default(),
    );
    push_text_property_change(
        changes,
        node,
        ChangedProperty::Value,
        previous.value.as_deref().unwrap_or_default(),
        current.value.as_deref().unwrap_or_default(),
    );
    push_property_change(
        changes,
        node,
        ChangedProperty::Enabled,
        PropertyValue::Bool(previous.enabled),
        PropertyValue::Bool(current.enabled),
    );
    push_property_change(
        changes,
        node,
        ChangedProperty::Focused,
        PropertyValue::Bool(previous.focused),
        PropertyValue::Bool(current.focused),
    );
    push_property_change(
        changes,
        node,
        ChangedProperty::Offscreen,
        PropertyValue::Bool(previous.offscreen),
        PropertyValue::Bool(current.offscreen),
    );
    push_property_change(
        changes,
        node,
        ChangedProperty::ControlType,
        PropertyValue::Integer(control_type_id(previous.control_type)),
        PropertyValue::Integer(control_type_id(current.control_type)),
    );
    push_property_change(
        changes,
        node,
        ChangedProperty::BoundingRectangle,
        PropertyValue::Rectangle(previous.bounds.unwrap_or_default()),
        PropertyValue::Rectangle(current.bounds.unwrap_or_default()),
    );

    let previous_invoke = has_invoke(previous);
    let current_invoke = has_invoke(current);
    push_property_change(
        changes,
        node,
        ChangedProperty::InvokeAvailable,
        PropertyValue::Bool(previous_invoke),
        PropertyValue::Bool(current_invoke),
    );

    let previous_toggle = toggle_pattern(previous);
    let current_toggle = toggle_pattern(current);
    push_property_change(
        changes,
        node,
        ChangedProperty::ToggleAvailable,
        PropertyValue::Bool(previous_toggle.is_some()),
        PropertyValue::Bool(current_toggle.is_some()),
    );
    if let (Some(previous), Some(current)) = (previous_toggle, current_toggle) {
        push_property_change(
            changes,
            node,
            ChangedProperty::ToggleState,
            PropertyValue::Integer(toggle_state_value(previous)),
            PropertyValue::Integer(toggle_state_value(current)),
        );
    }

    let previous_range = range_pattern(previous);
    let current_range = range_pattern(current);
    push_property_change(
        changes,
        node,
        ChangedProperty::RangeValueAvailable,
        PropertyValue::Bool(previous_range.is_some()),
        PropertyValue::Bool(current_range.is_some()),
    );
    if let (Some(previous), Some(current)) = (previous_range, current_range) {
        for (property, previous, current) in [
            (
                ChangedProperty::RangeValue,
                PropertyValue::Number(previous.value),
                PropertyValue::Number(current.value),
            ),
            (
                ChangedProperty::RangeMinimum,
                PropertyValue::Number(previous.minimum),
                PropertyValue::Number(current.minimum),
            ),
            (
                ChangedProperty::RangeMaximum,
                PropertyValue::Number(previous.maximum),
                PropertyValue::Number(current.maximum),
            ),
            (
                ChangedProperty::RangeSmallChange,
                PropertyValue::Number(previous.small_change),
                PropertyValue::Number(current.small_change),
            ),
            (
                ChangedProperty::RangeLargeChange,
                PropertyValue::Number(previous.large_change),
                PropertyValue::Number(current.large_change),
            ),
            (
                ChangedProperty::RangeReadOnly,
                PropertyValue::Bool(previous.read_only),
                PropertyValue::Bool(current.read_only),
            ),
        ] {
            push_property_change(changes, node, property, previous, current);
        }
    }
}

fn push_property_change(
    changes: &mut Vec<PropertyChange>,
    node: SettingsAccessibilityNodeId,
    property: ChangedProperty,
    old_value: PropertyValue,
    new_value: PropertyValue,
) {
    if old_value != new_value {
        changes.push(PropertyChange {
            node,
            property,
            old_value,
            new_value,
        });
    }
}

fn push_text_property_change(
    changes: &mut Vec<PropertyChange>,
    node: SettingsAccessibilityNodeId,
    property: ChangedProperty,
    old_value: &str,
    new_value: &str,
) {
    if old_value != new_value {
        changes.push(PropertyChange {
            node,
            property,
            old_value: PropertyValue::Text(old_value.to_owned()),
            new_value: PropertyValue::Text(new_value.to_owned()),
        });
    }
}

const fn toggle_state_value(checked: bool) -> i32 {
    if checked {
        ToggleState_On.0
    } else {
        ToggleState_Off.0
    }
}

/// Replaces the Settings accessibility tree with one complete immutable frame.
pub(super) fn publish(
    hwnd: HWND,
    scene: &SettingsScene,
    layout: &SettingsLayout,
    dpi: Dpi,
) -> Result<()> {
    let snapshot = settings_accessibility_snapshot(scene, layout)
        .map_err(|error| Error::new(E_UNEXPECTED, error.to_string()))?;
    let mut origin = POINT::default();
    // SAFETY: `hwnd` is the live Settings HWND and `origin` is writable storage.
    if !unsafe { ClientToScreen(hwnd, &mut origin) }.as_bool() {
        return Err(Error::from_thread());
    }
    let native = NativeSnapshot::from_renderer(&snapshot, origin, dpi);
    let state = {
        let providers = PROVIDERS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut providers = lock_recover(providers);
        providers
            .entry(hwnd.0 as isize)
            .or_insert_with(|| Arc::new(ProviderState::new(hwnd)))
            .clone()
    };
    state.active.store(true, Ordering::Release);
    let (previous, current) = state.replace(native);
    if let Some(previous) = previous {
        // SAFETY: This documented process-wide predicate performs no callback
        // and is evaluated on the HWND owner thread immediately after a full
        // immutable frame has been published.
        if unsafe { UiaClientsAreListening() }.as_bool() {
            let changes = diff_snapshots(&previous, &current);
            raise_snapshot_events(&state, changes);
        }
    }
    Ok(())
}

fn raise_snapshot_events(state: &Arc<ProviderState>, changes: SnapshotDiff) {
    if !state.active.load(Ordering::Acquire) {
        return;
    }

    // Structure comes first so a client can refresh its tree before observing
    // property or focus changes on elements in that tree. ChildrenInvalidated
    // intentionally carries no AppendRuntimeId pseudo-ID.
    for parent in changes.invalidated_parents {
        let Some(provider) = event_provider(state, parent) else {
            continue;
        };
        // SAFETY: The provider is alive in the newly published snapshot. The
        // documented ChildrenInvalidated form requires a null runtime ID.
        let _ = unsafe {
            UiaRaiseStructureChangedEvent(
                &provider,
                StructureChangeType_ChildrenInvalidated,
                std::ptr::null_mut(),
                0,
            )
        };
    }

    for change in changes.properties {
        let Some(provider) = event_provider(state, change.node) else {
            continue;
        };
        let (Ok(old_value), Ok(new_value)) = (
            change.old_value.into_variant(),
            change.new_value.into_variant(),
        ) else {
            continue;
        };
        // SAFETY: Both VARIANTs own valid values for this documented UIA
        // property and remain alive for the synchronous call.
        let _ = unsafe {
            UiaRaiseAutomationPropertyChangedEvent(
                &provider,
                change.property.uia_id(),
                &old_value,
                &new_value,
            )
        };
    }

    if let Some(focused) = changes.focused
        && let Some(provider) = event_provider(state, focused)
    {
        // SAFETY: The provider represents the focused node in the currently
        // published snapshot and remains alive for this synchronous call.
        let _ = unsafe { UiaRaiseAutomationEvent(&provider, UIA_AutomationFocusChangedEventId) };
    }
}

fn event_provider(
    state: &Arc<ProviderState>,
    id: SettingsAccessibilityNodeId,
) -> Option<IRawElementProviderSimple> {
    let snapshot = state.current().ok()?;
    snapshot.node(id)?;
    let raw: ISettingsRawElementProviderSimple =
        SettingsElementProvider::new(state.clone(), id).into();
    raw.cast().ok()
}

/// Removes a destroyed Settings HWND from discovery and invalidates providers
/// that an automation client may still retain.
pub(super) fn unregister(hwnd: HWND) {
    let Some(providers) = PROVIDERS.get() else {
        return;
    };
    if let Some(state) = lock_recover(providers).remove(&(hwnd.0 as isize)) {
        state.active.store(false, Ordering::Release);
        *lock_recover(&state.snapshot) = None;
    }
    // SAFETY: A null provider for this HWND is the documented UIA cache-clear
    // notification during destruction. The call carries no application pointer.
    let _ = unsafe {
        UiaReturnRawElementProvider(
            hwnd,
            WPARAM(0),
            LPARAM(0),
            None::<&IRawElementProviderSimple>,
        )
    };
}

/// Returns a UIA root only for the documented `UiaRootObjectId` request.
pub(super) fn handle_wm_getobject(hwnd: HWND, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    if lparam.0 as i32 != UiaRootObjectId {
        return None;
    }
    let state = PROVIDERS
        .get()
        .and_then(|providers| lock_recover(providers).get(&(hwnd.0 as isize)).cloned())?;
    state.current().ok()?;
    let raw: ISettingsRawElementProviderSimple =
        SettingsElementProvider::new(state, SettingsAccessibilityNodeId::Root).into();
    let provider = raw.cast::<IRawElementProviderSimple>().ok()?;
    // SAFETY: The provider is a ref-counted COM object and remains alive for
    // the synchronous call. UIA takes the reference it needs before returning.
    Some(unsafe { UiaReturnRawElementProvider(hwnd, wparam, lparam, &provider) })
}

#[implement(
    ISettingsRawElementProviderSimple,
    ISettingsRawElementProviderFragment,
    ISettingsRawElementProviderFragmentRoot,
    IInvokeProvider,
    IToggleProvider,
    IRangeValueProvider
)]
struct SettingsElementProvider {
    state: Arc<ProviderState>,
    id: SettingsAccessibilityNodeId,
}

impl SettingsElementProvider {
    fn new(state: Arc<ProviderState>, id: SettingsAccessibilityNodeId) -> Self {
        Self { state, id }
    }

    fn node(&self) -> Result<NativeNode> {
        self.state
            .current()?
            .node(self.id)
            .cloned()
            .ok_or_else(element_not_available_error)
    }

    fn fragment(&self, id: SettingsAccessibilityNodeId) -> Result<IRawElementProviderFragment> {
        let raw: ISettingsRawElementProviderFragment = Self::new(self.state.clone(), id).into();
        raw.cast()
    }

    fn root(&self) -> Result<IRawElementProviderFragmentRoot> {
        let raw: ISettingsRawElementProviderFragmentRoot =
            Self::new(self.state.clone(), SettingsAccessibilityNodeId::Root).into();
        raw.cast()
    }

    fn ensure_enabled(&self) -> Result<NativeNode> {
        let node = self.node()?;
        if !node.enabled {
            return element_not_enabled();
        }
        Ok(node)
    }

    fn invoke_action(&self, node: &NativeNode) -> Option<SettingsAutomationAction> {
        let (hit, expected_section) = match node.id {
            SettingsAccessibilityNodeId::NavigationItem(section) => {
                (SettingsHit::Navigation(section), None)
            }
            SettingsAccessibilityNodeId::Control { section, control } => {
                (SettingsHit::Control(control), Some(section))
            }
            SettingsAccessibilityNodeId::Back => (
                SettingsHit::Back,
                active_section_for_node(node, &self.state),
            ),
            SettingsAccessibilityNodeId::Reset => (SettingsHit::Reset, None),
            SettingsAccessibilityNodeId::Cancel => (SettingsHit::Cancel, None),
            SettingsAccessibilityNodeId::Apply => (SettingsHit::Apply, None),
            _ => return None,
        };
        Some(SettingsAutomationAction::Invoke {
            hit,
            expected_section,
        })
    }

    fn focus_action(&self, node: &NativeNode) -> Option<SettingsAutomationAction> {
        let (focus, expected_section) = match node.id {
            SettingsAccessibilityNodeId::NavigationItem(section) => {
                (SettingsFocus::Navigation(section), None)
            }
            SettingsAccessibilityNodeId::Control { section, control } => {
                (SettingsFocus::Control(control), Some(section))
            }
            SettingsAccessibilityNodeId::Back => (
                SettingsFocus::Back,
                active_section_for_node(node, &self.state),
            ),
            SettingsAccessibilityNodeId::Reset => (SettingsFocus::Reset, None),
            SettingsAccessibilityNodeId::Cancel => (SettingsFocus::Cancel, None),
            SettingsAccessibilityNodeId::Apply => (SettingsFocus::Apply, None),
            _ => return None,
        };
        Some(SettingsAutomationAction::Focus {
            focus,
            expected_section,
        })
    }
}

#[allow(non_snake_case)]
impl ISettingsRawElementProviderSimple_Impl for SettingsElementProvider_Impl {
    unsafe fn ProviderOptions(&self, result: *mut ProviderOptions) -> HRESULT {
        // SAFETY: The raw provider ABI requires a writable out-parameter; the
        // helper validates it before transferring the copy.
        unsafe {
            write_out(
                result,
                ProviderOptions_ServerSideProvider
                    | ProviderOptions_ProviderOwnsSetFocus
                    | ProviderOptions_UseComThreading,
            )
        }
    }

    unsafe fn GetPatternProvider(
        &self,
        patternid: UIA_PATTERN_ID,
        result: *mut *mut c_void,
    ) -> HRESULT {
        if result.is_null() {
            return E_POINTER;
        }
        let node = match self.node() {
            Ok(node) => node,
            Err(error) => return error.code(),
        };
        let provider = if patternid == UIA_InvokePatternId && has_invoke(&node) {
            let provider: IInvokeProvider =
                SettingsElementProvider::new(self.state.clone(), self.id).into();
            Some(provider.into_raw())
        } else if patternid == UIA_TogglePatternId && toggle_pattern(&node).is_some() {
            let provider: IToggleProvider =
                SettingsElementProvider::new(self.state.clone(), self.id).into();
            Some(provider.into_raw())
        } else if patternid == UIA_RangeValuePatternId && range_pattern(&node).is_some() {
            let provider: IRangeValueProvider =
                SettingsElementProvider::new(self.state.clone(), self.id).into();
            Some(provider.into_raw())
        } else {
            None
        };
        // SAFETY: Every interface pointer above transfers one owned COM
        // reference to UIA; unsupported patterns explicitly write null.
        unsafe { write_raw_interface(result, provider) }
    }

    unsafe fn GetPropertyValue(
        &self,
        propertyid: UIA_PROPERTY_ID,
        result: *mut VARIANT,
    ) -> HRESULT {
        let node = match self.node() {
            Ok(node) => node,
            Err(error) => return error.code(),
        };
        let value = if propertyid == UIA_NamePropertyId {
            VARIANT::from(node.name.as_str())
        } else if propertyid == UIA_HelpTextPropertyId {
            VARIANT::from(node.help_text.as_deref().unwrap_or(""))
        } else if propertyid == UIA_ValueValuePropertyId {
            VARIANT::from(node.value.as_deref().unwrap_or(""))
        } else if propertyid == UIA_RangeValueValuePropertyId {
            range_pattern(&node).map_or_else(VARIANT::default, |range| VARIANT::from(range.value))
        } else if propertyid == UIA_IsEnabledPropertyId {
            VARIANT::from(node.enabled)
        } else if propertyid == UIA_HasKeyboardFocusPropertyId {
            VARIANT::from(node.focused)
        } else if propertyid == UIA_IsOffscreenPropertyId {
            VARIANT::from(node.offscreen)
        } else if propertyid == UIA_ControlTypePropertyId {
            VARIANT::from(control_type_id(node.control_type))
        } else {
            VARIANT::default()
        };
        // SAFETY: `write_out` validates the COM out-parameter and transfers the
        // VARIANT, including any owned BSTR, without an intermediate drop.
        unsafe { write_out(result, value) }
    }

    unsafe fn HostRawElementProvider(&self, result: *mut *mut c_void) -> HRESULT {
        if result.is_null() {
            return E_POINTER;
        }
        let provider = if self.id == SettingsAccessibilityNodeId::Root {
            // SAFETY: The provider state verifies the Settings HWND has not
            // been unregistered. UIA owns the returned interface reference.
            match unsafe { UiaHostProviderFromHwnd(self.state.hwnd()) } {
                Ok(provider) => Some(provider.into_raw()),
                Err(error) => return error.code(),
            }
        } else {
            None
        };
        // SAFETY: The helper initializes the raw interface out-parameter even
        // when this child has no host provider.
        unsafe { write_raw_interface(result, provider) }
    }
}

#[allow(non_snake_case)]
impl ISettingsRawElementProviderFragment_Impl for SettingsElementProvider_Impl {
    unsafe fn Navigate(&self, direction: NavigateDirection, result: *mut *mut c_void) -> HRESULT {
        if result.is_null() {
            return E_POINTER;
        }
        let snapshot = match self.state.current() {
            Ok(snapshot) => snapshot,
            Err(error) => return error.code(),
        };
        let provider = match snapshot.navigate(self.id, direction) {
            Some(id) => match self.fragment(id) {
                Ok(provider) => Some(provider.into_raw()),
                Err(error) => return error.code(),
            },
            None => None,
        };
        // SAFETY: The optional interface owns one reference and null is the
        // documented result at a fragment-tree boundary.
        unsafe { write_raw_interface(result, provider) }
    }

    unsafe fn GetRuntimeId(&self, result: *mut *mut SAFEARRAY) -> HRESULT {
        if result.is_null() {
            return E_POINTER;
        }
        let id = match runtime_id(self.id) {
            Ok(id) => id,
            Err(error) => return error.code(),
        };
        // SAFETY: `id` is an owned SAFEARRAY transferred to the UIA caller.
        unsafe { write_out(result, id) }
    }

    unsafe fn BoundingRectangle(&self, result: *mut UiaRect) -> HRESULT {
        let bounds = match self.node() {
            Ok(node) => node.bounds.unwrap_or_default(),
            Err(error) => return error.code(),
        };
        // SAFETY: `write_out` validates and initializes the COM out-parameter.
        unsafe { write_out(result, bounds) }
    }

    unsafe fn GetEmbeddedFragmentRoots(&self, result: *mut *mut SAFEARRAY) -> HRESULT {
        // SAFETY: Settings embeds no other fragment roots; UIA requires a
        // successful, explicitly null SAFEARRAY out-parameter.
        unsafe { write_out(result, std::ptr::null_mut()) }
    }

    unsafe fn SetFocus(&self) -> HRESULT {
        let node = match self.ensure_enabled() {
            Ok(node) => node,
            Err(error) => return error.code(),
        };
        let Some(action) = self.focus_action(&node) else {
            return invalid_operation_error().code();
        };
        match self.state.queue(action) {
            Ok(()) => HRESULT(0),
            Err(error) => error.code(),
        }
    }

    unsafe fn FragmentRoot(&self, result: *mut *mut c_void) -> HRESULT {
        if result.is_null() {
            return E_POINTER;
        }
        if let Err(error) = self.state.current() {
            return error.code();
        }
        let provider = match self.root() {
            Ok(provider) => provider.into_raw(),
            Err(error) => return error.code(),
        };
        // SAFETY: The fragment-root pointer transfers one owned COM reference.
        unsafe { write_raw_interface(result, Some(provider)) }
    }
}

#[allow(non_snake_case)]
impl ISettingsRawElementProviderFragmentRoot_Impl for SettingsElementProvider_Impl {
    unsafe fn ElementProviderFromPoint(&self, x: f64, y: f64, result: *mut *mut c_void) -> HRESULT {
        if result.is_null() {
            return E_POINTER;
        }
        let snapshot = match self.state.current() {
            Ok(snapshot) => snapshot,
            Err(error) => return error.code(),
        };
        let provider = match snapshot.element_from_point(x, y) {
            Some(id) => match self.fragment(id) {
                Ok(provider) => Some(provider.into_raw()),
                Err(error) => return error.code(),
            },
            None => None,
        };
        // SAFETY: No hit is represented by a successful null out-parameter.
        unsafe { write_raw_interface(result, provider) }
    }

    unsafe fn GetFocus(&self, result: *mut *mut c_void) -> HRESULT {
        if result.is_null() {
            return E_POINTER;
        }
        let snapshot = match self.state.current() {
            Ok(snapshot) => snapshot,
            Err(error) => return error.code(),
        };
        let provider = match snapshot.focused() {
            Some(id) => match self.fragment(id) {
                Ok(provider) => Some(provider.into_raw()),
                Err(error) => return error.code(),
            },
            None => None,
        };
        // SAFETY: No focused fragment is represented by a successful null.
        unsafe { write_raw_interface(result, provider) }
    }
}

#[allow(non_snake_case)]
impl IInvokeProvider_Impl for SettingsElementProvider_Impl {
    fn Invoke(&self) -> Result<()> {
        let node = self.ensure_enabled()?;
        if !has_invoke(&node) {
            return invalid_operation();
        }
        let Some(action) = self.invoke_action(&node) else {
            return invalid_operation();
        };
        self.state.queue(action)
    }
}

#[allow(non_snake_case)]
impl IToggleProvider_Impl for SettingsElementProvider_Impl {
    fn Toggle(&self) -> Result<()> {
        let node = self.ensure_enabled()?;
        if toggle_pattern(&node).is_none() {
            return invalid_operation();
        }
        let Some(action) = self.invoke_action(&node) else {
            return invalid_operation();
        };
        self.state.queue(action)
    }

    fn ToggleState(&self) -> Result<ToggleState> {
        let node = self.node()?;
        toggle_pattern(&node)
            .map(|checked| {
                if checked {
                    ToggleState_On
                } else {
                    ToggleState_Off
                }
            })
            .ok_or_else(invalid_operation_error)
    }
}

#[allow(non_snake_case)]
impl IRangeValueProvider_Impl for SettingsElementProvider_Impl {
    fn SetValue(&self, val: f64) -> Result<()> {
        if val.is_nan() {
            return invalid_operation();
        }
        let node = self.ensure_enabled()?;
        let Some(range) = range_pattern(&node) else {
            return invalid_operation();
        };
        if range.read_only {
            return invalid_operation();
        }
        let SettingsAccessibilityNodeId::Control { section, control } = node.id else {
            return invalid_operation();
        };
        self.state.queue(SettingsAutomationAction::SetRange {
            section,
            control,
            position: val,
        })
    }

    fn Value(&self) -> Result<f64> {
        range_pattern(&self.node()?)
            .map(|range| range.value)
            .ok_or_else(invalid_operation_error)
    }

    fn IsReadOnly(&self) -> Result<BOOL> {
        range_pattern(&self.node()?)
            .map(|range| BOOL::from(range.read_only))
            .ok_or_else(invalid_operation_error)
    }

    fn Maximum(&self) -> Result<f64> {
        range_pattern(&self.node()?)
            .map(|range| range.maximum)
            .ok_or_else(invalid_operation_error)
    }

    fn Minimum(&self) -> Result<f64> {
        range_pattern(&self.node()?)
            .map(|range| range.minimum)
            .ok_or_else(invalid_operation_error)
    }

    fn LargeChange(&self) -> Result<f64> {
        range_pattern(&self.node()?)
            .map(|range| range.large_change)
            .ok_or_else(invalid_operation_error)
    }

    fn SmallChange(&self) -> Result<f64> {
        range_pattern(&self.node()?)
            .map(|range| range.small_change)
            .ok_or_else(invalid_operation_error)
    }
}

#[derive(Clone, Copy)]
struct RangePattern {
    value: f64,
    minimum: f64,
    maximum: f64,
    small_change: f64,
    large_change: f64,
    read_only: bool,
}

fn has_invoke(node: &NativeNode) -> bool {
    node.patterns
        .contains(&SettingsAccessibilityPattern::Invoke)
}

fn toggle_pattern(node: &NativeNode) -> Option<bool> {
    node.patterns.iter().find_map(|pattern| match *pattern {
        SettingsAccessibilityPattern::Toggle { checked } => Some(checked),
        _ => None,
    })
}

fn range_pattern(node: &NativeNode) -> Option<RangePattern> {
    node.patterns.iter().find_map(|pattern| match *pattern {
        SettingsAccessibilityPattern::RangeValue {
            value,
            minimum,
            maximum,
            small_change,
            large_change,
            read_only,
        } => Some(RangePattern {
            value,
            minimum,
            maximum,
            small_change,
            large_change,
            read_only,
        }),
        _ => None,
    })
}

fn active_section_for_node(_node: &NativeNode, state: &ProviderState) -> Option<SettingsSectionId> {
    state.current().ok()?.nodes.iter().find_map(|candidate| {
        if let SettingsAccessibilityNodeId::ContentHeading(section) = candidate.id {
            Some(section)
        } else {
            None
        }
    })
}

fn control_type_id(control_type: SettingsAccessibilityControlType) -> i32 {
    match control_type {
        SettingsAccessibilityControlType::Window => UIA_WindowControlTypeId.0,
        SettingsAccessibilityControlType::Group => UIA_GroupControlTypeId.0,
        SettingsAccessibilityControlType::List => UIA_ListControlTypeId.0,
        SettingsAccessibilityControlType::ListItem => UIA_ListItemControlTypeId.0,
        SettingsAccessibilityControlType::Text => UIA_TextControlTypeId.0,
        SettingsAccessibilityControlType::Button => UIA_ButtonControlTypeId.0,
        SettingsAccessibilityControlType::CheckBox => UIA_CheckBoxControlTypeId.0,
        SettingsAccessibilityControlType::Slider => UIA_SliderControlTypeId.0,
        SettingsAccessibilityControlType::ComboBox => UIA_ComboBoxControlTypeId.0,
        SettingsAccessibilityControlType::Edit => UIA_EditControlTypeId.0,
    }
}

fn screen_rect(bounds: shell_renderer::DipRect, origin: POINT, dpi: Dpi) -> UiaRect {
    let scale = f64::from(dpi.raw()) / 96.0;
    UiaRect {
        left: f64::from(origin.x) + f64::from(bounds.x) * scale,
        top: f64::from(origin.y) + f64::from(bounds.y) * scale,
        width: f64::from(bounds.width) * scale,
        height: f64::from(bounds.height) * scale,
    }
}

fn contains(bounds: UiaRect, x: f64, y: f64) -> bool {
    x >= bounds.left
        && x < bounds.left + bounds.width
        && y >= bounds.top
        && y < bounds.top + bounds.height
}

fn rectangle_variant(rectangle: UiaRect) -> Result<VARIANT> {
    let values = [
        rectangle.left,
        rectangle.top,
        rectangle.width,
        rectangle.height,
    ];
    // SAFETY: The SAFEARRAY has exactly four VT_R8 slots. Every index written
    // is in range, and failure destroys the still-owned array. On success the
    // returned VARIANT owns the array and VariantClear releases it on drop.
    unsafe {
        let array = SafeArrayCreateVector(VT_R8, 0, values.len() as u32);
        if array.is_null() {
            return Err(Error::from_thread());
        }
        for (index, value) in values.iter().enumerate() {
            let index = index as i32;
            if let Err(error) =
                SafeArrayPutElement(array, &index, (value as *const f64).cast::<c_void>())
            {
                let _ = SafeArrayDestroy(array);
                return Err(error);
            }
        }
        Ok(VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_ARRAY | VT_R8,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 { parray: array },
                }),
            },
        })
    }
}

fn runtime_id(id: SettingsAccessibilityNodeId) -> Result<*mut SAFEARRAY> {
    let components = runtime_id_components(id);
    // SAFETY: The array has exactly `components.len()` VT_I4 slots. Every
    // written index is in range; on a failed write we destroy the owned array.
    unsafe {
        let array = SafeArrayCreateVector(VT_I4, 0, components.len() as u32);
        if array.is_null() {
            return Err(Error::from_thread());
        }
        for (index, value) in components.iter().copied().enumerate() {
            let index = index as i32;
            if let Err(error) =
                SafeArrayPutElement(array, &index, (&value as *const i32).cast::<c_void>())
            {
                let _ = SafeArrayDestroy(array);
                return Err(error);
            }
        }
        Ok(array)
    }
}

fn runtime_id_components(id: SettingsAccessibilityNodeId) -> [i32; 6] {
    let (tag, section, control) = match id {
        SettingsAccessibilityNodeId::Root => (1, 0, 0),
        SettingsAccessibilityNodeId::Back => (2, 0, 0),
        SettingsAccessibilityNodeId::Title => (3, 0, 0),
        SettingsAccessibilityNodeId::Navigation => (4, 0, 0),
        SettingsAccessibilityNodeId::NavigationItem(section) => (5, section.value(), 0),
        SettingsAccessibilityNodeId::Content => (6, 0, 0),
        SettingsAccessibilityNodeId::ContentHeading(section) => (7, section.value(), 0),
        SettingsAccessibilityNodeId::Control { section, control } => {
            (8, section.value(), control.value())
        }
        SettingsAccessibilityNodeId::Footer => (9, 0, 0),
        SettingsAccessibilityNodeId::Status => (10, 0, 0),
        SettingsAccessibilityNodeId::Reset => (11, 0, 0),
        SettingsAccessibilityNodeId::Cancel => (12, 0, 0),
        SettingsAccessibilityNodeId::Apply => (13, 0, 0),
    };
    [
        UiaAppendRuntimeId as i32,
        tag,
        section as u32 as i32,
        (section >> 32) as u32 as i32,
        control as u32 as i32,
        (control >> 32) as u32 as i32,
    ]
}

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            mutex.clear_poison();
            poisoned.into_inner()
        }
    }
}

unsafe fn write_out<T>(result: *mut T, value: T) -> HRESULT {
    if result.is_null() {
        return E_POINTER;
    }
    // SAFETY: The caller supplied a non-null COM out-parameter for `T`. Writing
    // transfers `value` to that storage exactly once.
    unsafe { result.write(value) };
    HRESULT(0)
}

unsafe fn write_raw_interface(result: *mut *mut c_void, value: Option<*mut c_void>) -> HRESULT {
    // SAFETY: `write_out` validates the outer pointer. The optional raw pointer
    // is either null or an owned COM reference already transferred by `into_raw`.
    unsafe { write_out(result, value.unwrap_or(std::ptr::null_mut())) }
}

fn element_not_available_error() -> Error {
    Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32))
}

fn element_not_available<T>() -> Result<T> {
    Err(element_not_available_error())
}

fn element_not_enabled<T>() -> Result<T> {
    Err(Error::from_hresult(HRESULT(UIA_E_ELEMENTNOTENABLED as i32)))
}

fn invalid_operation_error() -> Error {
    Error::from_hresult(HRESULT(UIA_E_INVALIDOPERATION as i32))
}

fn invalid_operation<T>() -> Result<T> {
    Err(invalid_operation_error())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shell_renderer::{
        DipRect, SettingsControl, SettingsControlId, SettingsControlKind, SettingsNavigationItem,
    };

    fn fixture_snapshot() -> NativeSnapshot {
        let section = SettingsSectionId::new(1);
        let scene = SettingsScene::from_navigation(
            "Settings",
            vec![SettingsNavigationItem::new(
                section,
                "Dock",
                "Dock settings",
                true,
            )],
        )
        .with_active_section(Some(section))
        .with_controls(vec![
            SettingsControl::new(
                SettingsControlId::new(1),
                "Size",
                "Icon size",
                "50",
                SettingsControlKind::Slider { position: 50 },
                true,
                false,
            ),
            SettingsControl::new(
                SettingsControlId::new(2),
                "Hide",
                "Hide Dock",
                "Off",
                SettingsControlKind::Toggle { checked: false },
                true,
                false,
            ),
        ])
        .with_focus(Some(SettingsFocus::Control(SettingsControlId::new(2))));
        let surface = DipRect::new(0.0, 0.0, 992.0, 620.0);
        let layout = shell_renderer::layout_settings_scene(&scene, surface);
        let snapshot = settings_accessibility_snapshot(&scene, &layout).expect("valid fixture");
        NativeSnapshot::from_renderer(&snapshot, POINT { x: 100, y: 200 }, Dpi::from_raw(96))
    }

    fn node_mut(snapshot: &mut NativeSnapshot, id: SettingsAccessibilityNodeId) -> &mut NativeNode {
        snapshot
            .nodes
            .iter_mut()
            .find(|node| node.id == id)
            .expect("fixture node")
    }

    #[test]
    fn identical_published_snapshots_do_not_emit_changes() {
        let snapshot = fixture_snapshot();

        assert_eq!(
            diff_snapshots(&snapshot, &snapshot),
            SnapshotDiff::default()
        );
    }

    #[test]
    fn snapshot_diff_reports_property_and_focus_changes_without_tree_churn() {
        let previous = fixture_snapshot();
        let mut current = previous.clone();
        let section = SettingsSectionId::new(1);
        let slider = SettingsAccessibilityNodeId::Control {
            section,
            control: SettingsControlId::new(1),
        };
        let toggle = SettingsAccessibilityNodeId::Control {
            section,
            control: SettingsControlId::new(2),
        };

        node_mut(&mut current, toggle).focused = false;
        let slider_node = node_mut(&mut current, slider);
        slider_node.focused = true;
        slider_node.value = Some("64".to_owned());
        let range = slider_node
            .patterns
            .iter_mut()
            .find(|pattern| matches!(pattern, SettingsAccessibilityPattern::RangeValue { .. }))
            .expect("slider range pattern");
        *range = SettingsAccessibilityPattern::RangeValue {
            value: 64.0,
            minimum: 0.0,
            maximum: 100.0,
            small_change: 1.0,
            large_change: 10.0,
            read_only: false,
        };

        let changes = diff_snapshots(&previous, &current);

        assert!(changes.invalidated_parents.is_empty());
        assert_eq!(changes.focused, Some(slider));
        assert!(changes.properties.iter().any(|change| {
            change.node == slider && change.property == ChangedProperty::RangeValue
        }));
        assert!(changes.properties.iter().any(|change| {
            change.node == slider && change.property == ChangedProperty::Focused
        }));
        assert!(changes.properties.iter().any(|change| {
            change.node == toggle && change.property == ChangedProperty::Focused
        }));
    }

    #[test]
    fn snapshot_diff_invalidates_the_surviving_parent_for_removed_children() {
        let previous = fixture_snapshot();
        let mut current = previous.clone();
        let removed = SettingsAccessibilityNodeId::Control {
            section: SettingsSectionId::new(1),
            control: SettingsControlId::new(2),
        };
        current.nodes = current
            .nodes
            .iter()
            .filter(|node| node.id != removed)
            .cloned()
            .collect::<Vec<_>>()
            .into_boxed_slice();

        let changes = diff_snapshots(&previous, &current);

        assert_eq!(
            changes.invalidated_parents,
            vec![SettingsAccessibilityNodeId::Content]
        );
        assert_eq!(changes.focused, None);
    }

    #[test]
    fn deactivated_provider_state_rejects_retained_elements() {
        let state = ProviderState::new(HWND::default());
        let _ = state.replace(fixture_snapshot());

        state.active.store(false, Ordering::Release);
        *lock_recover(&state.snapshot) = None;

        let error = match state.current() {
            Ok(_) => panic!("destroyed HWND must invalidate UIA"),
            Err(error) => error,
        };
        assert_eq!(error.code(), HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32));
    }

    #[test]
    fn bounding_rectangle_property_uses_a_four_double_variant() {
        let variant = rectangle_variant(UiaRect {
            left: 10.0,
            top: 20.0,
            width: 30.0,
            height: 40.0,
        })
        .expect("bounding rectangle variant");

        assert_eq!(variant.vt(), VT_ARRAY | VT_R8);
    }

    #[test]
    fn composite_control_runtime_ids_do_not_collide_across_sections() {
        let control = SettingsControlId::new(1);
        let dock = runtime_id_components(SettingsAccessibilityNodeId::Control {
            section: SettingsSectionId::new(1),
            control,
        });
        let quick_controls = runtime_id_components(SettingsAccessibilityNodeId::Control {
            section: SettingsSectionId::new(4),
            control,
        });

        assert_ne!(dock, quick_controls);
        assert_eq!(dock[0], UiaAppendRuntimeId as i32);
    }

    #[test]
    fn raw_provider_interfaces_match_windows_uia_identity_and_vtable_size() {
        use windows::Win32::UI::Accessibility::{
            IRawElementProviderFragment_Vtbl, IRawElementProviderFragmentRoot_Vtbl,
            IRawElementProviderSimple_Vtbl,
        };

        assert_eq!(
            ISettingsRawElementProviderSimple::IID,
            IRawElementProviderSimple::IID
        );
        assert_eq!(
            ISettingsRawElementProviderFragment::IID,
            IRawElementProviderFragment::IID
        );
        assert_eq!(
            ISettingsRawElementProviderFragmentRoot::IID,
            IRawElementProviderFragmentRoot::IID
        );
        assert_eq!(
            std::mem::size_of::<ISettingsRawElementProviderSimple_Vtbl>(),
            std::mem::size_of::<IRawElementProviderSimple_Vtbl>()
        );
        assert_eq!(
            std::mem::size_of::<ISettingsRawElementProviderFragment_Vtbl>(),
            std::mem::size_of::<IRawElementProviderFragment_Vtbl>()
        );
        assert_eq!(
            std::mem::size_of::<ISettingsRawElementProviderFragmentRoot_Vtbl>(),
            std::mem::size_of::<IRawElementProviderFragmentRoot_Vtbl>()
        );
    }

    #[test]
    fn dip_bounds_are_scaled_and_translated_to_screen_pixels() {
        let bounds = screen_rect(
            DipRect::new(10.0, 20.0, 30.0, 40.0),
            POINT { x: -200, y: 100 },
            Dpi::from_raw(144),
        );

        assert_eq!(bounds.left, -185.0);
        assert_eq!(bounds.top, 130.0);
        assert_eq!(bounds.width, 45.0);
        assert_eq!(bounds.height, 60.0);
    }

    #[test]
    fn navigation_preserves_parent_first_last_and_sibling_links() {
        let snapshot = fixture_snapshot();
        let section = SettingsSectionId::new(1);
        let first_control = SettingsAccessibilityNodeId::Control {
            section,
            control: SettingsControlId::new(1),
        };
        let last_control = SettingsAccessibilityNodeId::Control {
            section,
            control: SettingsControlId::new(2),
        };

        assert_eq!(
            snapshot.navigate(
                SettingsAccessibilityNodeId::Content,
                NavigateDirection_FirstChild
            ),
            Some(SettingsAccessibilityNodeId::ContentHeading(section))
        );
        assert_eq!(
            snapshot.navigate(
                SettingsAccessibilityNodeId::Content,
                NavigateDirection_LastChild
            ),
            Some(last_control)
        );
        assert_eq!(
            snapshot.navigate(first_control, NavigateDirection_Parent),
            Some(SettingsAccessibilityNodeId::Content)
        );
        assert_eq!(
            snapshot.navigate(first_control, NavigateDirection_NextSibling),
            Some(last_control)
        );
        assert_eq!(
            snapshot.navigate(last_control, NavigateDirection_PreviousSibling),
            Some(first_control)
        );
    }

    #[test]
    fn point_lookup_selects_deepest_node_and_focus_uses_semantic_state() {
        let snapshot = fixture_snapshot();
        let focused = SettingsAccessibilityNodeId::Control {
            section: SettingsSectionId::new(1),
            control: SettingsControlId::new(2),
        };
        let bounds = snapshot
            .node(focused)
            .and_then(|node| node.bounds)
            .expect("focused control bounds");

        assert_eq!(snapshot.focused(), Some(focused));
        assert_eq!(
            snapshot.element_from_point(bounds.left + 1.0, bounds.top + 1.0),
            Some(focused)
        );
        assert_eq!(snapshot.element_from_point(-10_000.0, -10_000.0), None);
    }

    #[test]
    fn raw_optional_interface_out_parameter_is_always_initialized() {
        let mut result = std::ptr::dangling_mut::<c_void>();

        // SAFETY: `result` is valid writable storage for one raw interface
        // pointer and the helper receives no owned interface reference.
        let status = unsafe { write_raw_interface(&mut result, None) };

        assert_eq!(status, HRESULT(0));
        assert!(result.is_null());
        // SAFETY: This deliberately passes a null outer pointer to verify the
        // ABI guard; the helper checks it before writing.
        let null_status = unsafe { write_raw_interface(std::ptr::null_mut(), None) };
        assert_eq!(null_status, E_POINTER);
    }
}
