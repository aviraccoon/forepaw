//! Windows UIA `ControlType` to cross-platform `Role` mapping.

use crate::core::role::Role;

/// UIA `ControlType` IDs mapped to `Role` variants.
///
/// Names follow the cross-platform convention so `Role::is_interactive()` and the
/// tree renderer work cross-platform. UIA types without a close
/// equivalent map to the nearest structural equivalent or `Role::Unknown`.
///
/// Source: <https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-controltype-ids>
#[must_use]
pub fn control_type_to_role(control_type: i32) -> Role {
    match control_type {
        50000 => Role::Button,                             // Button
        50001 => Role::Calendar,                           // Calendar
        50002 => Role::CheckBox,                           // CheckBox
        50003 => Role::ComboBox,                           // ComboBox
        50004 => Role::TextField,                          // Edit
        50005 => Role::Link,                               // Hyperlink
        50006 => Role::Image,                              // Image
        50007 | 50029 => Role::Cell,                       // ListItem, DataItem
        50008 => Role::List,                               // List
        50009 => Role::Menu,                               // Menu
        50010 => Role::MenuBar,                            // MenuBar
        50011 => Role::MenuItem,                           // MenuItem
        50012 => Role::ProgressIndicator,                  // ProgressBar
        50013 => Role::RadioButton,                        // RadioButton
        50014 => Role::ScrollBar,                          // ScrollBar
        50015 => Role::Slider,                             // Slider
        50016 => Role::Incrementor,                        // Spinner
        50017 | 50020 | 50035 | 50037 => Role::StaticText, // StatusBar, Text, HeaderItem, TitleBar
        50018 => Role::TabGroup,                           // Tab (container)
        50019 => Role::Tab,                                // TabItem
        50021 => Role::Toolbar,                            // ToolBar
        50023 => Role::Outline,                            // Tree
        50024 => Role::TreeItem,                           // TreeItem
        50026 | 50033 | 50034 | 50041 => Role::Group,      // Group, Pane, Header, Pane (dup)
        50028 | 50036 => Role::Table,                      // DataGrid, Table
        50030 => Role::TextArea,                           // Document
        50031 => Role::MenuButton,                         // SplitButton
        50032 => Role::Window,                             // Window
        // ToolTip(50022), Custom(50025), Thumb(50027), Separator(50038),
        // SemanticZoom(50039), AppBar(50040), and anything else
        _ => Role::Unknown,
    }
}
