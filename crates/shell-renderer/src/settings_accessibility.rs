//! Provider-neutral accessibility semantics for the Settings surface.
//!
//! This module deliberately stops at an immutable logical snapshot. The native
//! UI Automation provider remains responsible for handling `WM_GETOBJECT`,
//! converting DIP bounds into screen coordinates, executing pattern actions,
//! and raising focus/property/structure events.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use crate::{
    DipRect, SettingsControl, SettingsControlId, SettingsControlKind, SettingsFocus,
    SettingsLayout, SettingsNavigationItem, SettingsScene, SettingsSectionId,
};

/// Stable, provider-neutral identity for one logical Settings element.
///
/// Identities are derived from typed scene identities rather than layout
/// position, so redraws, scrolling, DPI changes, and responsive resize do not
/// renumber existing elements.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SettingsAccessibilityNodeId {
    Root,
    Back,
    Title,
    Navigation,
    NavigationItem(SettingsSectionId),
    Content,
    ContentHeading(SettingsSectionId),
    Control {
        section: SettingsSectionId,
        control: SettingsControlId,
    },
    Footer,
    Status,
    Reset,
    Cancel,
    Apply,
}

/// Provider-neutral control type intent for a Settings accessibility node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsAccessibilityControlType {
    Window,
    Group,
    List,
    ListItem,
    Text,
    Button,
    CheckBox,
    Slider,
    ComboBox,
    Edit,
}

/// Interaction pattern that a future native provider must expose and execute.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SettingsAccessibilityPattern {
    Invoke,
    Toggle {
        checked: bool,
    },
    RangeValue {
        value: f64,
        minimum: f64,
        maximum: f64,
        small_change: f64,
        large_change: f64,
        read_only: bool,
    },
    Value {
        read_only: bool,
    },
}

/// One immutable node in a Settings accessibility snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsAccessibilityNode {
    id: SettingsAccessibilityNodeId,
    parent: Option<SettingsAccessibilityNodeId>,
    previous_sibling: Option<SettingsAccessibilityNodeId>,
    next_sibling: Option<SettingsAccessibilityNodeId>,
    logical_order: usize,
    name: String,
    help_text: Option<String>,
    value: Option<String>,
    enabled: bool,
    focused: bool,
    offscreen: bool,
    control_type: SettingsAccessibilityControlType,
    patterns: Box<[SettingsAccessibilityPattern]>,
    bounds_dip: Option<DipRect>,
}

impl SettingsAccessibilityNode {
    #[must_use]
    pub const fn id(&self) -> SettingsAccessibilityNodeId {
        self.id
    }

    #[must_use]
    pub const fn parent(&self) -> Option<SettingsAccessibilityNodeId> {
        self.parent
    }

    #[must_use]
    pub const fn previous_sibling(&self) -> Option<SettingsAccessibilityNodeId> {
        self.previous_sibling
    }

    #[must_use]
    pub const fn next_sibling(&self) -> Option<SettingsAccessibilityNodeId> {
        self.next_sibling
    }

    #[must_use]
    pub const fn logical_order(&self) -> usize {
        self.logical_order
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn help_text(&self) -> Option<&str> {
        self.help_text.as_deref()
    }

    #[must_use]
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn focused(&self) -> bool {
        self.focused
    }

    #[must_use]
    pub const fn offscreen(&self) -> bool {
        self.offscreen
    }

    #[must_use]
    pub const fn control_type(&self) -> SettingsAccessibilityControlType {
        self.control_type
    }

    #[must_use]
    pub fn patterns(&self) -> &[SettingsAccessibilityPattern] {
        &self.patterns
    }

    /// Returns painted bounds in device-independent pixels.
    ///
    /// Offscreen logical items intentionally return `None`; the accessibility
    /// layer never fabricates a clickable rectangle for an item omitted by the
    /// current layout viewport.
    #[must_use]
    pub const fn bounds_dip(&self) -> Option<DipRect> {
        self.bounds_dip
    }
}

/// Immutable logical tree consumed by a future native UI Automation provider.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsAccessibilitySnapshot {
    nodes: Box<[SettingsAccessibilityNode]>,
}

impl SettingsAccessibilitySnapshot {
    #[must_use]
    pub fn nodes(&self) -> &[SettingsAccessibilityNode] {
        &self.nodes
    }

    #[must_use]
    pub fn node(&self, id: SettingsAccessibilityNodeId) -> Option<&SettingsAccessibilityNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn children_of(
        &self,
        parent: SettingsAccessibilityNodeId,
    ) -> impl Iterator<Item = &SettingsAccessibilityNode> {
        self.nodes
            .iter()
            .filter(move |node| node.parent == Some(parent))
    }
}

/// A scene cannot form a valid accessibility tree when semantic IDs collide.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingsAccessibilityError {
    duplicate: SettingsAccessibilityNodeId,
}

impl SettingsAccessibilityError {
    #[must_use]
    pub const fn duplicate(&self) -> SettingsAccessibilityNodeId {
        self.duplicate
    }
}

impl fmt::Display for SettingsAccessibilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "duplicate Settings accessibility node identity: {:?}",
            self.duplicate
        )
    }
}

impl Error for SettingsAccessibilityError {}

/// Projects renderer state into provider-neutral Settings accessibility data.
///
/// Every logical navigation item and control from `scene` is included. A row
/// outside the current `layout` viewport remains in tree order with
/// `offscreen = true` and no bounds. This function does not install a native UI
/// Automation provider or make the window discoverable through `WM_GETOBJECT`.
pub fn settings_accessibility_snapshot(
    scene: &SettingsScene,
    layout: &SettingsLayout,
) -> Result<SettingsAccessibilitySnapshot, SettingsAccessibilityError> {
    let mut builder = SnapshotBuilder::default();
    let root_bounds = union_bounds([
        visible_bounds(Some(layout.title_bounds())),
        visible_bounds(layout.navigation_bounds()),
        visible_bounds(layout.content_bounds()),
        visible_bounds(layout.back_bounds()),
        visible_bounds(layout.reset_bounds()),
        visible_bounds(layout.status_bounds()),
        visible_bounds(layout.cancel_bounds()),
        visible_bounds(layout.apply_bounds()),
    ]);
    builder.push(NodeSeed::new(
        SettingsAccessibilityNodeId::Root,
        None,
        scene.title(),
        SettingsAccessibilityControlType::Window,
        root_bounds,
    ))?;

    if let Some(bounds) = visible_bounds(layout.back_bounds()) {
        builder.push(
            NodeSeed::new(
                SettingsAccessibilityNodeId::Back,
                Some(SettingsAccessibilityNodeId::Root),
                "Back",
                SettingsAccessibilityControlType::Button,
                Some(bounds),
            )
            .with_help_text("Return to Settings sections.")
            .with_focused(scene.focus() == Some(SettingsFocus::Back))
            .with_patterns(vec![SettingsAccessibilityPattern::Invoke]),
        )?;
    }

    builder.push(NodeSeed::new(
        SettingsAccessibilityNodeId::Title,
        Some(SettingsAccessibilityNodeId::Root),
        scene.title(),
        SettingsAccessibilityControlType::Text,
        visible_bounds(Some(layout.title_bounds())),
    ))?;

    let navigation_bounds = visible_bounds(layout.navigation_bounds());
    builder.push(NodeSeed::new(
        SettingsAccessibilityNodeId::Navigation,
        Some(SettingsAccessibilityNodeId::Root),
        "Settings sections",
        SettingsAccessibilityControlType::List,
        navigation_bounds,
    ))?;
    for item in scene.navigation() {
        push_navigation_item(&mut builder, scene, layout, item)?;
    }

    if let Some(section) = scene.active_section() {
        let content_bounds = visible_bounds(layout.content_bounds());
        let content_name = scene
            .navigation_item(section)
            .map_or("Settings content", SettingsNavigationItem::label);
        builder.push(NodeSeed::new(
            SettingsAccessibilityNodeId::Content,
            Some(SettingsAccessibilityNodeId::Root),
            content_name,
            SettingsAccessibilityControlType::Group,
            content_bounds,
        ))?;

        if let Some(item) = scene.navigation_item(section) {
            builder.push(
                NodeSeed::new(
                    SettingsAccessibilityNodeId::ContentHeading(section),
                    Some(SettingsAccessibilityNodeId::Content),
                    item.label(),
                    SettingsAccessibilityControlType::Text,
                    visible_bounds(layout.content_heading_bounds()),
                )
                .with_help_text(item.detail()),
            )?;
        }

        for control in scene.controls() {
            push_control(&mut builder, scene, layout, control)?;
        }
    }

    let footer_bounds = union_bounds([
        visible_bounds(layout.reset_bounds()),
        visible_bounds(layout.status_bounds()),
        visible_bounds(layout.cancel_bounds()),
        visible_bounds(layout.apply_bounds()),
    ]);
    builder.push(NodeSeed::new(
        SettingsAccessibilityNodeId::Footer,
        Some(SettingsAccessibilityNodeId::Root),
        "Settings actions",
        SettingsAccessibilityControlType::Group,
        footer_bounds,
    ))?;
    builder.push(
        NodeSeed::new(
            SettingsAccessibilityNodeId::Reset,
            Some(SettingsAccessibilityNodeId::Footer),
            "Reset",
            SettingsAccessibilityControlType::Button,
            visible_bounds(layout.reset_bounds()),
        )
        .with_help_text("Restore default Settings values.")
        .with_focused(scene.focus() == Some(SettingsFocus::Reset))
        .with_patterns(vec![SettingsAccessibilityPattern::Invoke]),
    )?;
    if !scene.status_text().is_empty() {
        builder.push(
            NodeSeed::new(
                SettingsAccessibilityNodeId::Status,
                Some(SettingsAccessibilityNodeId::Footer),
                "Settings status",
                SettingsAccessibilityControlType::Text,
                visible_bounds(layout.status_bounds()),
            )
            .with_value(scene.status_text()),
        )?;
    }
    builder.push(
        NodeSeed::new(
            SettingsAccessibilityNodeId::Cancel,
            Some(SettingsAccessibilityNodeId::Footer),
            "Cancel",
            SettingsAccessibilityControlType::Button,
            visible_bounds(layout.cancel_bounds()),
        )
        .with_help_text("Discard pending Settings changes.")
        .with_enabled(scene.dirty())
        .with_focused(scene.focus() == Some(SettingsFocus::Cancel))
        .with_patterns(vec![SettingsAccessibilityPattern::Invoke]),
    )?;
    builder.push(
        NodeSeed::new(
            SettingsAccessibilityNodeId::Apply,
            Some(SettingsAccessibilityNodeId::Footer),
            "Apply",
            SettingsAccessibilityControlType::Button,
            visible_bounds(layout.apply_bounds()),
        )
        .with_help_text("Save pending Settings changes.")
        .with_enabled(scene.dirty())
        .with_focused(scene.focus() == Some(SettingsFocus::Apply))
        .with_patterns(vec![SettingsAccessibilityPattern::Invoke]),
    )?;

    Ok(builder.finish())
}

fn push_navigation_item(
    builder: &mut SnapshotBuilder,
    scene: &SettingsScene,
    layout: &SettingsLayout,
    item: &SettingsNavigationItem,
) -> Result<(), SettingsAccessibilityError> {
    let bounds = layout
        .navigation()
        .iter()
        .find(|laid_out| laid_out.section() == item.section())
        .and_then(|laid_out| visible_bounds(Some(laid_out.bounds())));
    builder.push(
        NodeSeed::new(
            SettingsAccessibilityNodeId::NavigationItem(item.section()),
            Some(SettingsAccessibilityNodeId::Navigation),
            item.label(),
            SettingsAccessibilityControlType::ListItem,
            bounds,
        )
        .with_help_text(item.detail())
        .with_enabled(item.enabled())
        .with_focused(scene.focus() == Some(SettingsFocus::Navigation(item.section())))
        .with_patterns(vec![SettingsAccessibilityPattern::Invoke]),
    )
}

fn push_control(
    builder: &mut SnapshotBuilder,
    scene: &SettingsScene,
    layout: &SettingsLayout,
    control: &SettingsControl,
) -> Result<(), SettingsAccessibilityError> {
    let Some(section) = scene.active_section() else {
        return Ok(());
    };
    let bounds = layout
        .controls()
        .iter()
        .find(|laid_out| laid_out.id() == control.id())
        .and_then(|laid_out| visible_bounds(Some(laid_out.bounds())));
    let (control_type, patterns) = control_semantics(control);
    builder.push(
        NodeSeed::new(
            SettingsAccessibilityNodeId::Control {
                section,
                control: control.id(),
            },
            Some(SettingsAccessibilityNodeId::Content),
            control.label(),
            control_type,
            bounds,
        )
        .with_help_text(control.detail())
        .with_optional_value(optional_text(control.value()))
        .with_enabled(control.enabled())
        .with_focused(scene.focus() == Some(SettingsFocus::Control(control.id())))
        .with_patterns(patterns),
    )
}

fn control_semantics(
    control: &SettingsControl,
) -> (
    SettingsAccessibilityControlType,
    Vec<SettingsAccessibilityPattern>,
) {
    match control.kind() {
        SettingsControlKind::Toggle { checked } => (
            SettingsAccessibilityControlType::CheckBox,
            vec![SettingsAccessibilityPattern::Toggle { checked }],
        ),
        SettingsControlKind::Slider { position } => (
            SettingsAccessibilityControlType::Slider,
            vec![SettingsAccessibilityPattern::RangeValue {
                value: f64::from(position),
                minimum: 0.0,
                maximum: 100.0,
                small_change: 1.0,
                large_change: 10.0,
                read_only: !control.enabled(),
            }],
        ),
        SettingsControlKind::Choice => (
            SettingsAccessibilityControlType::ComboBox,
            vec![SettingsAccessibilityPattern::Invoke],
        ),
        SettingsControlKind::Action => (
            SettingsAccessibilityControlType::Button,
            vec![SettingsAccessibilityPattern::Invoke],
        ),
        SettingsControlKind::Text | SettingsControlKind::Path | SettingsControlKind::Shortcut => (
            SettingsAccessibilityControlType::Edit,
            vec![SettingsAccessibilityPattern::Value {
                read_only: !control.enabled(),
            }],
        ),
        SettingsControlKind::ReadOnly | SettingsControlKind::Reorder => (
            SettingsAccessibilityControlType::Text,
            vec![SettingsAccessibilityPattern::Value { read_only: true }],
        ),
    }
}

fn optional_text(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

fn visible_bounds(bounds: Option<DipRect>) -> Option<DipRect> {
    bounds.filter(|bounds| {
        bounds.x.is_finite()
            && bounds.y.is_finite()
            && bounds.width.is_finite()
            && bounds.height.is_finite()
            && bounds.width > 0.0
            && bounds.height > 0.0
    })
}

fn union_bounds<const N: usize>(bounds: [Option<DipRect>; N]) -> Option<DipRect> {
    bounds.into_iter().flatten().reduce(|left, right| {
        let x = left.x.min(right.x);
        let y = left.y.min(right.y);
        let right_edge = (left.x + left.width).max(right.x + right.width);
        let bottom = (left.y + left.height).max(right.y + right.height);
        DipRect::new(x, y, right_edge - x, bottom - y)
    })
}

struct NodeSeed<'a> {
    id: SettingsAccessibilityNodeId,
    parent: Option<SettingsAccessibilityNodeId>,
    name: &'a str,
    help_text: Option<&'a str>,
    value: Option<&'a str>,
    enabled: bool,
    focused: bool,
    control_type: SettingsAccessibilityControlType,
    patterns: Vec<SettingsAccessibilityPattern>,
    bounds_dip: Option<DipRect>,
}

impl<'a> NodeSeed<'a> {
    const fn new(
        id: SettingsAccessibilityNodeId,
        parent: Option<SettingsAccessibilityNodeId>,
        name: &'a str,
        control_type: SettingsAccessibilityControlType,
        bounds_dip: Option<DipRect>,
    ) -> Self {
        Self {
            id,
            parent,
            name,
            help_text: None,
            value: None,
            enabled: true,
            focused: false,
            control_type,
            patterns: Vec::new(),
            bounds_dip,
        }
    }

    fn with_help_text(mut self, help_text: &'a str) -> Self {
        self.help_text = optional_text(help_text);
        self
    }

    const fn with_value(mut self, value: &'a str) -> Self {
        self.value = Some(value);
        self
    }

    const fn with_optional_value(mut self, value: Option<&'a str>) -> Self {
        self.value = value;
        self
    }

    const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    const fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    fn with_patterns(mut self, patterns: Vec<SettingsAccessibilityPattern>) -> Self {
        self.patterns = patterns;
        self
    }
}

#[derive(Default)]
struct SnapshotBuilder {
    nodes: Vec<SettingsAccessibilityNode>,
    indices: HashMap<SettingsAccessibilityNodeId, usize>,
    last_child: HashMap<SettingsAccessibilityNodeId, SettingsAccessibilityNodeId>,
}

impl SnapshotBuilder {
    fn push(&mut self, seed: NodeSeed<'_>) -> Result<(), SettingsAccessibilityError> {
        if self.indices.contains_key(&seed.id) {
            return Err(SettingsAccessibilityError { duplicate: seed.id });
        }
        let previous_sibling = seed
            .parent
            .and_then(|parent| self.last_child.get(&parent).copied());
        if let Some(previous) = previous_sibling
            && let Some(index) = self.indices.get(&previous).copied()
            && let Some(node) = self.nodes.get_mut(index)
        {
            node.next_sibling = Some(seed.id);
        }
        let logical_order = self.nodes.len();
        let index = logical_order;
        self.nodes.push(SettingsAccessibilityNode {
            id: seed.id,
            parent: seed.parent,
            previous_sibling,
            next_sibling: None,
            logical_order,
            name: seed.name.to_owned(),
            help_text: seed.help_text.map(str::to_owned),
            value: seed.value.map(str::to_owned),
            enabled: seed.enabled,
            focused: seed.focused,
            offscreen: seed.bounds_dip.is_none(),
            control_type: seed.control_type,
            patterns: seed.patterns.into_boxed_slice(),
            bounds_dip: seed.bounds_dip,
        });
        self.indices.insert(seed.id, index);
        if let Some(parent) = seed.parent {
            self.last_child.insert(parent, seed.id);
        }
        Ok(())
    }

    fn finish(self) -> SettingsAccessibilitySnapshot {
        SettingsAccessibilitySnapshot {
            nodes: self.nodes.into_boxed_slice(),
        }
    }
}
