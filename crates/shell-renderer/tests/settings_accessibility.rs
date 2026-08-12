use std::collections::HashSet;

use crate::{
    DipRect, SettingsAccessibilityControlType, SettingsAccessibilityNode,
    SettingsAccessibilityNodeId, SettingsAccessibilityPattern, SettingsAccessibilitySnapshot,
    SettingsControl, SettingsControlId, SettingsControlKind, SettingsFocus, SettingsNavigationItem,
    SettingsScene, SettingsSectionId, layout_settings_scene, settings_accessibility_snapshot,
};

const DOCK: SettingsSectionId = SettingsSectionId::new(1);
const TOPBAR: SettingsSectionId = SettingsSectionId::new(2);
const APPEARANCE: SettingsSectionId = SettingsSectionId::new(3);
const QUICK_CONTROLS: SettingsSectionId = SettingsSectionId::new(4);
const SIZE: SettingsControlId = SettingsControlId::new(101);
const MAGNIFICATION: SettingsControlId = SettingsControlId::new(102);

fn navigation() -> Vec<SettingsNavigationItem> {
    vec![
        SettingsNavigationItem::new(DOCK, "Dock", "Size and behavior", true),
        SettingsNavigationItem::new(TOPBAR, "Top bar", "Modules and density", true),
        SettingsNavigationItem::new(APPEARANCE, "Appearance", "Surface and color", true),
    ]
}

fn controls() -> Vec<SettingsControl> {
    vec![
        SettingsControl::new(
            SIZE,
            "Icon size",
            "Changes the resting size of Dock icons.",
            "48 DIP",
            SettingsControlKind::Slider { position: 50 },
            true,
            true,
        ),
        SettingsControl::new(
            MAGNIFICATION,
            "Magnification",
            "Enlarge nearby icons while pointing at the Dock.",
            "On",
            SettingsControlKind::Toggle { checked: true },
            true,
            false,
        ),
    ]
}

fn active_scene(dirty: bool) -> SettingsScene {
    SettingsScene::from_navigation("Settings", navigation())
        .with_active_section(Some(DOCK))
        .with_controls(controls())
        .with_focus(Some(SettingsFocus::Control(SIZE)))
        .with_dirty(dirty)
        .with_status_text("Previewing changes")
}

fn snapshot(scene: &SettingsScene, surface: DipRect) -> SettingsAccessibilitySnapshot {
    let layout = layout_settings_scene(scene, surface);
    settings_accessibility_snapshot(scene, &layout).expect("valid Settings accessibility snapshot")
}

fn node(
    snapshot: &SettingsAccessibilitySnapshot,
    id: SettingsAccessibilityNodeId,
) -> &SettingsAccessibilityNode {
    snapshot.node(id).expect("Settings accessibility node")
}

fn ids(snapshot: &SettingsAccessibilitySnapshot) -> Vec<SettingsAccessibilityNodeId> {
    snapshot
        .nodes()
        .iter()
        .map(SettingsAccessibilityNode::id)
        .collect()
}

fn child_ids(
    snapshot: &SettingsAccessibilitySnapshot,
    parent: SettingsAccessibilityNodeId,
) -> Vec<SettingsAccessibilityNodeId> {
    snapshot
        .children_of(parent)
        .map(SettingsAccessibilityNode::id)
        .collect()
}

#[test]
fn semantic_ids_survive_redraw_resize_and_scroll() {
    let scene = active_scene(true);
    let first = snapshot(&scene, DipRect::new(0.0, 0.0, 992.0, 620.0));
    let redraw = snapshot(&scene, DipRect::new(0.0, 0.0, 992.0, 620.0));
    let resized = snapshot(&scene, DipRect::new(0.0, 0.0, 1_320.0, 760.0));
    let scrolled_scene = scene.clone().with_scroll_offsets(1, 1);
    let scrolled = snapshot(&scrolled_scene, DipRect::new(0.0, 0.0, 992.0, 620.0));

    assert_eq!(ids(&first), ids(&redraw));
    assert_eq!(ids(&first), ids(&resized));
    assert_eq!(ids(&first), ids(&scrolled));

    let compact = snapshot(&scene, DipRect::new(0.0, 0.0, 680.0, 620.0));
    let wide_ids = ids(&first).into_iter().collect::<HashSet<_>>();
    let compact_ids = ids(&compact).into_iter().collect::<HashSet<_>>();
    assert!(compact_ids.contains(&SettingsAccessibilityNodeId::Back));
    assert!(wide_ids.is_subset(&compact_ids));
    assert_eq!(
        node(
            &compact,
            SettingsAccessibilityNodeId::Control {
                section: DOCK,
                control: MAGNIFICATION,
            }
        )
        .id(),
        SettingsAccessibilityNodeId::Control {
            section: DOCK,
            control: MAGNIFICATION,
        }
    );
}

#[test]
fn equal_raw_control_ids_in_different_sections_have_distinct_semantic_ids() {
    let raw_control = SettingsControlId::new(1);
    let control_for = |label: &str| {
        SettingsControl::new(
            raw_control,
            label,
            "Section-specific setting.",
            "On",
            SettingsControlKind::Toggle { checked: true },
            true,
            false,
        )
    };
    let section_navigation = || {
        vec![
            SettingsNavigationItem::new(DOCK, "Dock", "Size and behavior", true),
            SettingsNavigationItem::new(
                QUICK_CONTROLS,
                "Quick Controls",
                "Visible controls and order",
                true,
            ),
        ]
    };
    let dock_scene = SettingsScene::from_navigation("Settings", section_navigation())
        .with_active_section(Some(DOCK))
        .with_controls(vec![control_for("Dock setting")]);
    let quick_controls_scene = SettingsScene::from_navigation("Settings", section_navigation())
        .with_active_section(Some(QUICK_CONTROLS))
        .with_controls(vec![control_for("Quick control setting")]);
    let surface = DipRect::new(0.0, 0.0, 992.0, 620.0);
    let dock = snapshot(&dock_scene, surface);
    let quick_controls = snapshot(&quick_controls_scene, surface);
    let dock_id = SettingsAccessibilityNodeId::Control {
        section: DOCK,
        control: raw_control,
    };
    let quick_controls_id = SettingsAccessibilityNodeId::Control {
        section: QUICK_CONTROLS,
        control: raw_control,
    };

    assert_ne!(dock_id, quick_controls_id);
    assert_eq!(node(&dock, dock_id).name(), "Dock setting");
    assert_eq!(
        node(&quick_controls, quick_controls_id).name(),
        "Quick control setting"
    );
    assert!(dock.node(quick_controls_id).is_none());
    assert!(quick_controls.node(dock_id).is_none());
}

#[test]
fn ids_tree_links_and_logical_order_are_unique_and_reciprocal() {
    let snapshot = snapshot(&active_scene(true), DipRect::new(0.0, 0.0, 992.0, 620.0));
    let mut unique = HashSet::new();

    for (index, current) in snapshot.nodes().iter().enumerate() {
        assert!(unique.insert(current.id()));
        assert_eq!(current.logical_order(), index);
        if let Some(parent) = current.parent() {
            assert!(snapshot.node(parent).is_some());
        }
        if let Some(previous) = current.previous_sibling() {
            assert_eq!(node(&snapshot, previous).next_sibling(), Some(current.id()));
            assert_eq!(node(&snapshot, previous).parent(), current.parent());
        }
        if let Some(next) = current.next_sibling() {
            assert_eq!(node(&snapshot, next).previous_sibling(), Some(current.id()));
            assert_eq!(node(&snapshot, next).parent(), current.parent());
        }
    }
}

#[test]
fn wide_and_narrow_trees_follow_visual_reading_order() {
    let scene = active_scene(true);
    let wide = snapshot(&scene, DipRect::new(0.0, 0.0, 992.0, 620.0));
    let compact = snapshot(&scene, DipRect::new(0.0, 0.0, 680.0, 620.0));

    assert_eq!(
        child_ids(&wide, SettingsAccessibilityNodeId::Root),
        vec![
            SettingsAccessibilityNodeId::Title,
            SettingsAccessibilityNodeId::Navigation,
            SettingsAccessibilityNodeId::Content,
            SettingsAccessibilityNodeId::Footer,
        ]
    );
    assert_eq!(
        child_ids(&compact, SettingsAccessibilityNodeId::Root),
        vec![
            SettingsAccessibilityNodeId::Back,
            SettingsAccessibilityNodeId::Title,
            SettingsAccessibilityNodeId::Navigation,
            SettingsAccessibilityNodeId::Content,
            SettingsAccessibilityNodeId::Footer,
        ]
    );
    let expected_navigation = vec![
        SettingsAccessibilityNodeId::NavigationItem(DOCK),
        SettingsAccessibilityNodeId::NavigationItem(TOPBAR),
        SettingsAccessibilityNodeId::NavigationItem(APPEARANCE),
    ];
    assert_eq!(
        child_ids(&wide, SettingsAccessibilityNodeId::Navigation),
        expected_navigation
    );
    assert_eq!(
        child_ids(&compact, SettingsAccessibilityNodeId::Navigation),
        expected_navigation
    );
    assert_eq!(
        child_ids(&wide, SettingsAccessibilityNodeId::Content),
        vec![
            SettingsAccessibilityNodeId::ContentHeading(DOCK),
            SettingsAccessibilityNodeId::Control {
                section: DOCK,
                control: SIZE,
            },
            SettingsAccessibilityNodeId::Control {
                section: DOCK,
                control: MAGNIFICATION,
            },
        ]
    );
    assert!(node(&compact, SettingsAccessibilityNodeId::Navigation).offscreen());
    for item in compact.children_of(SettingsAccessibilityNodeId::Navigation) {
        assert!(item.offscreen());
        assert_eq!(item.bounds_dip(), None);
    }
}

#[test]
fn metadata_and_pattern_intents_cover_settings_control_kinds() {
    let choice = SettingsControlId::new(201);
    let action = SettingsControlId::new(202);
    let toggle = SettingsControlId::new(203);
    let slider = SettingsControlId::new(204);
    let read_only = SettingsControlId::new(205);
    let scene = SettingsScene::from_navigation("Settings", navigation())
        .with_active_section(Some(DOCK))
        .with_controls(vec![
            SettingsControl::new(
                choice,
                "Alignment",
                "Align the Dock within the monitor.",
                "Center",
                SettingsControlKind::Choice,
                true,
                false,
            ),
            SettingsControl::new(
                action,
                "Open folder",
                "Open the selected folder.",
                "",
                SettingsControlKind::Action,
                true,
                false,
            ),
            SettingsControl::new(
                toggle,
                "Automatically hide",
                "Reveal from the monitor edge.",
                "On",
                SettingsControlKind::Toggle { checked: true },
                true,
                false,
            ),
            SettingsControl::new(
                slider,
                "Opacity",
                "Surface tint strength.",
                "100%",
                SettingsControlKind::Slider { position: 150 },
                true,
                false,
            ),
            SettingsControl::new(
                read_only,
                "Managed setting",
                "Managed by Windows.",
                "Windows default",
                SettingsControlKind::ReadOnly,
                false,
                false,
            ),
        ])
        .with_dirty(true)
        .with_focus(Some(SettingsFocus::Apply));
    let wide = snapshot(&scene, DipRect::new(0.0, 0.0, 992.0, 720.0));
    let compact = snapshot(&scene, DipRect::new(0.0, 0.0, 680.0, 720.0));

    let navigation = node(&wide, SettingsAccessibilityNodeId::NavigationItem(DOCK));
    assert_eq!(navigation.name(), "Dock");
    assert_eq!(navigation.help_text(), Some("Size and behavior"));
    assert_eq!(
        navigation.control_type(),
        SettingsAccessibilityControlType::ListItem
    );
    assert_eq!(
        navigation.patterns(),
        &[SettingsAccessibilityPattern::Invoke]
    );

    let choice = node(
        &wide,
        SettingsAccessibilityNodeId::Control {
            section: DOCK,
            control: choice,
        },
    );
    assert_eq!(choice.name(), "Alignment");
    assert_eq!(
        choice.help_text(),
        Some("Align the Dock within the monitor.")
    );
    assert_eq!(choice.value(), Some("Center"));
    assert_eq!(
        choice.control_type(),
        SettingsAccessibilityControlType::ComboBox
    );
    assert_eq!(choice.patterns(), &[SettingsAccessibilityPattern::Invoke]);

    let action = node(
        &wide,
        SettingsAccessibilityNodeId::Control {
            section: DOCK,
            control: action,
        },
    );
    assert_eq!(
        action.control_type(),
        SettingsAccessibilityControlType::Button
    );
    assert_eq!(action.value(), None);
    assert_eq!(action.patterns(), &[SettingsAccessibilityPattern::Invoke]);

    let toggle = node(
        &wide,
        SettingsAccessibilityNodeId::Control {
            section: DOCK,
            control: toggle,
        },
    );
    assert_eq!(
        toggle.control_type(),
        SettingsAccessibilityControlType::CheckBox
    );
    assert_eq!(
        toggle.patterns(),
        &[SettingsAccessibilityPattern::Toggle { checked: true }]
    );

    let slider = node(
        &wide,
        SettingsAccessibilityNodeId::Control {
            section: DOCK,
            control: slider,
        },
    );
    assert_eq!(
        slider.control_type(),
        SettingsAccessibilityControlType::Slider
    );
    assert_eq!(
        slider.patterns(),
        &[SettingsAccessibilityPattern::RangeValue {
            value: 100.0,
            minimum: 0.0,
            maximum: 100.0,
            small_change: 1.0,
            large_change: 10.0,
            read_only: false,
        }]
    );

    let read_only = node(
        &wide,
        SettingsAccessibilityNodeId::Control {
            section: DOCK,
            control: read_only,
        },
    );
    assert!(!read_only.enabled());
    assert_eq!(
        read_only.control_type(),
        SettingsAccessibilityControlType::Text
    );
    assert_eq!(
        read_only.patterns(),
        &[SettingsAccessibilityPattern::Value { read_only: true }]
    );

    for id in [
        SettingsAccessibilityNodeId::Back,
        SettingsAccessibilityNodeId::Reset,
        SettingsAccessibilityNodeId::Cancel,
        SettingsAccessibilityNodeId::Apply,
    ] {
        let source = if id == SettingsAccessibilityNodeId::Back {
            &compact
        } else {
            &wide
        };
        assert_eq!(
            node(source, id).patterns(),
            &[SettingsAccessibilityPattern::Invoke]
        );
    }
    assert!(node(&wide, SettingsAccessibilityNodeId::Apply).focused());
}

#[test]
fn disabled_and_clipped_items_remain_logical_but_have_no_fake_bounds() {
    let navigation = (0_u64..12)
        .map(|index| {
            SettingsNavigationItem::new(
                SettingsSectionId::new(index + 1),
                &format!("Section {index}"),
                "Section details",
                index != 2,
            )
        })
        .collect::<Vec<_>>();
    let controls = (0_u64..10)
        .map(|index| {
            SettingsControl::new(
                SettingsControlId::new(500 + index),
                &format!("Control {index}"),
                "Control details",
                &index.to_string(),
                SettingsControlKind::Slider {
                    position: u8::try_from(index * 10).unwrap_or(100),
                },
                index != 1,
                false,
            )
        })
        .collect::<Vec<_>>();
    let scene = SettingsScene::from_navigation("Settings", navigation)
        .with_active_section(Some(SettingsSectionId::new(1)))
        .with_controls(controls)
        .with_scroll_offsets(2, 1);
    let snapshot = snapshot(&scene, DipRect::new(0.0, 0.0, 992.0, 420.0));

    for section in 1_u64..=12 {
        assert!(
            snapshot
                .node(SettingsAccessibilityNodeId::NavigationItem(
                    SettingsSectionId::new(section)
                ))
                .is_some()
        );
    }
    for control in 500_u64..510 {
        assert!(
            snapshot
                .node(SettingsAccessibilityNodeId::Control {
                    section: SettingsSectionId::new(1),
                    control: SettingsControlId::new(control),
                })
                .is_some()
        );
    }

    let skipped_navigation = node(
        &snapshot,
        SettingsAccessibilityNodeId::NavigationItem(SettingsSectionId::new(1)),
    );
    assert!(skipped_navigation.offscreen());
    assert_eq!(skipped_navigation.bounds_dip(), None);
    let disabled_navigation = node(
        &snapshot,
        SettingsAccessibilityNodeId::NavigationItem(SettingsSectionId::new(3)),
    );
    assert!(!disabled_navigation.enabled());
    assert!(!disabled_navigation.offscreen());
    assert!(disabled_navigation.bounds_dip().is_some());

    let skipped_control = node(
        &snapshot,
        SettingsAccessibilityNodeId::Control {
            section: SettingsSectionId::new(1),
            control: SettingsControlId::new(500),
        },
    );
    assert!(skipped_control.offscreen());
    assert_eq!(skipped_control.bounds_dip(), None);
    let disabled_control = node(
        &snapshot,
        SettingsAccessibilityNodeId::Control {
            section: SettingsSectionId::new(1),
            control: SettingsControlId::new(501),
        },
    );
    assert!(!disabled_control.enabled());
    assert!(!disabled_control.offscreen());
    assert!(disabled_control.bounds_dip().is_some());
    let clipped_control = node(
        &snapshot,
        SettingsAccessibilityNodeId::Control {
            section: SettingsSectionId::new(1),
            control: SettingsControlId::new(509),
        },
    );
    assert!(clipped_control.offscreen());
    assert_eq!(clipped_control.bounds_dip(), None);

    assert!(!node(&snapshot, SettingsAccessibilityNodeId::Apply).enabled());
    assert!(!node(&snapshot, SettingsAccessibilityNodeId::Cancel).enabled());
}

#[test]
fn duplicate_semantic_ids_are_rejected_instead_of_ambiguously_exposed() {
    let duplicate = SettingsControlId::new(42);
    let control = || {
        SettingsControl::new(
            duplicate,
            "Duplicate",
            "Invalid duplicate semantic identity.",
            "On",
            SettingsControlKind::Toggle { checked: true },
            true,
            false,
        )
    };
    let scene = SettingsScene::from_navigation("Settings", navigation())
        .with_active_section(Some(DOCK))
        .with_controls(vec![control(), control()]);
    let layout = layout_settings_scene(&scene, DipRect::new(0.0, 0.0, 992.0, 620.0));
    let error = settings_accessibility_snapshot(&scene, &layout)
        .expect_err("duplicate semantic IDs must fail the snapshot");

    assert_eq!(
        error.duplicate(),
        SettingsAccessibilityNodeId::Control {
            section: DOCK,
            control: duplicate,
        }
    );
}
