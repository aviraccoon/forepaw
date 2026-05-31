/// Cross-platform accessibility element role.
///
/// One enum for all platforms. Each platform backend maps its native role type
/// (macOS `AXRole`, Windows `ControlType`, Linux AT-SPI2 `Role`) to a `Role`
/// variant. The [`Display`](std::fmt::Display) impl produces the lowercase
/// form used in tree output (`button`, `textfield`, etc.).
use std::fmt;

// ---------------------------------------------------------------------------
// Role enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    // -- Interactive (receive refs) --
    Button,
    TextField,
    TextArea,
    SecureTextField,
    CheckBox,
    RadioButton,
    Slider,
    ComboBox,
    PopUpButton,
    MenuButton,
    Link,
    MenuItem,
    MenuItemCheckBox,
    MenuItemRadio,
    Tab,
    Switch,
    Incrementor,
    ColorWell,
    TreeItem,
    Cell,
    DockItem,
    ScrollBar,

    // -- Structural (no refs) --
    Window,
    Application,
    Group,
    StaticText,
    Image,
    Menu,
    MenuBar,
    Toolbar,
    Table,
    List,
    Outline,
    TabGroup,
    ScrollArea,
    SplitGroup,
    Row,
    Column,
    ColumnHeader,
    RowHeader,
    Heading,
    Paragraph,
    Separator,
    StatusBar,
    Dialog,
    Alert,
    Frame,
    InternalFrame,
    WebArea,
    Tooltip,
    Calendar,
    DatePicker,
    ColorChooser,
    Icon,
    Label,
    ProgressIndicator,

    // -- Fallback --
    Unknown,
}

// ---------------------------------------------------------------------------
// Interactive check
// ---------------------------------------------------------------------------

impl Role {
    /// Whether this role should receive a ref during tree annotation.
    #[must_use]
    pub fn is_interactive(self) -> bool {
        matches!(
            self,
            Self::Button
                | Self::TextField
                | Self::TextArea
                | Self::SecureTextField
                | Self::CheckBox
                | Self::RadioButton
                | Self::Slider
                | Self::ComboBox
                | Self::PopUpButton
                | Self::MenuButton
                | Self::Link
                | Self::MenuItem
                | Self::MenuItemCheckBox
                | Self::MenuItemRadio
                | Self::Tab
                | Self::Switch
                | Self::Incrementor
                | Self::ColorWell
                | Self::TreeItem
                | Self::Cell
                | Self::DockItem
                | Self::ScrollBar
        )
    }

    /// Category for color-coding in screenshot annotations.
    #[must_use]
    pub fn annotation_category(self) -> AnnotationCategory {
        match self {
            Self::Button | Self::MenuButton | Self::DockItem | Self::Incrementor => {
                AnnotationCategory::Button
            }
            Self::TextField | Self::TextArea | Self::SecureTextField => {
                AnnotationCategory::TextInput
            }
            Self::CheckBox
            | Self::RadioButton
            | Self::Switch
            | Self::ComboBox
            | Self::PopUpButton
            | Self::Slider
            | Self::ColorWell => AnnotationCategory::Selection,
            Self::Link
            | Self::Tab
            | Self::MenuItem
            | Self::MenuItemCheckBox
            | Self::MenuItemRadio
            | Self::TreeItem => AnnotationCategory::Navigation,
            _ => AnnotationCategory::Other,
        }
    }

    /// Role label for display (title-case, no prefix).
    ///
    /// `"Button"`, `"TextField"`, etc. Used by annotation rendering.
    /// For lowercase output, use `self.to_string()` (`Display` impl).
    #[must_use]
    pub fn short_name(self) -> &'static str {
        // Debug gives "Button", "TextField", etc.
        // We strip "Unknown" variants — but all are plain names.
        match self {
            Self::Button => "Button",
            Self::TextField => "TextField",
            Self::TextArea => "TextArea",
            Self::SecureTextField => "SecureTextField",
            Self::CheckBox => "CheckBox",
            Self::RadioButton => "RadioButton",
            Self::Slider => "Slider",
            Self::ComboBox => "ComboBox",
            Self::PopUpButton => "PopUpButton",
            Self::MenuButton => "MenuButton",
            Self::Link => "Link",
            Self::MenuItem => "MenuItem",
            Self::MenuItemCheckBox => "MenuItemCheckBox",
            Self::MenuItemRadio => "MenuItemRadio",
            Self::Tab => "Tab",
            Self::Switch => "Switch",
            Self::Incrementor => "Incrementor",
            Self::ColorWell => "ColorWell",
            Self::TreeItem => "TreeItem",
            Self::Cell => "Cell",
            Self::DockItem => "DockItem",
            Self::ScrollBar => "ScrollBar",
            Self::Window => "Window",
            Self::Application => "Application",
            Self::Group => "Group",
            Self::StaticText => "StaticText",
            Self::Image => "Image",
            Self::Menu => "Menu",
            Self::MenuBar => "MenuBar",
            Self::Toolbar => "Toolbar",
            Self::Table => "Table",
            Self::List => "List",
            Self::Outline => "Outline",
            Self::TabGroup => "TabGroup",
            Self::ScrollArea => "ScrollArea",
            Self::SplitGroup => "SplitGroup",
            Self::Row => "Row",
            Self::Column => "Column",
            Self::ColumnHeader => "ColumnHeader",
            Self::RowHeader => "RowHeader",
            Self::Heading => "Heading",
            Self::Paragraph => "Paragraph",
            Self::Separator => "Separator",
            Self::StatusBar => "StatusBar",
            Self::Dialog => "Dialog",
            Self::Alert => "Alert",
            Self::Frame => "Frame",
            Self::InternalFrame => "InternalFrame",
            Self::WebArea => "WebArea",
            Self::Tooltip => "Tooltip",
            Self::Calendar => "Calendar",
            Self::DatePicker => "DatePicker",
            Self::ColorChooser => "ColorChooser",
            Self::Icon => "Icon",
            Self::Label => "Label",
            Self::ProgressIndicator => "ProgressIndicator",
            Self::Unknown => "Unknown",
        }
    }
}

// ---------------------------------------------------------------------------
// Display (lowercase for tree output)
// ---------------------------------------------------------------------------

impl serde::Serialize for Role {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.short_name())
    }
}

impl Role {
    /// Lowercase role name for prose (e.g. "button", "text field").
    ///
    /// Same output as [`short_name()`](Self::short_name). Use [`to_lowercase()`](Self::to_lowercase)
    /// for prose text (e.g. "the button is disabled").
    /// Use this for inline text: "the {role} is disabled".
    #[must_use]
    pub fn to_lowercase(self) -> String {
        self.short_name()
            .chars()
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Annotation category
// ---------------------------------------------------------------------------

/// Category for color-coding elements by type in screenshot annotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationCategory {
    Button,
    TextInput,
    Selection,
    Navigation,
    Other,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_roles() {
        assert!(Role::Button.is_interactive());
        assert!(Role::TextField.is_interactive());
        assert!(Role::CheckBox.is_interactive());
        assert!(Role::RadioButton.is_interactive());
        assert!(Role::Slider.is_interactive());
        assert!(Role::ComboBox.is_interactive());
        assert!(Role::PopUpButton.is_interactive());
        assert!(Role::MenuButton.is_interactive());
        assert!(Role::Link.is_interactive());
        assert!(Role::MenuItem.is_interactive());
        assert!(Role::Tab.is_interactive());
        assert!(Role::Switch.is_interactive());
        assert!(Role::Incrementor.is_interactive());
        assert!(Role::ColorWell.is_interactive());
        assert!(Role::TreeItem.is_interactive());
        assert!(Role::Cell.is_interactive());
        assert!(Role::DockItem.is_interactive());
        assert!(Role::ScrollBar.is_interactive());
        assert!(Role::TextArea.is_interactive());
        assert!(Role::SecureTextField.is_interactive());
        assert!(Role::MenuItemCheckBox.is_interactive());
        assert!(Role::MenuItemRadio.is_interactive());
    }

    #[test]
    fn structural_roles_not_interactive() {
        assert!(!Role::Window.is_interactive());
        assert!(!Role::Group.is_interactive());
        assert!(!Role::StaticText.is_interactive());
        assert!(!Role::Image.is_interactive());
        assert!(!Role::Menu.is_interactive());
        assert!(!Role::MenuBar.is_interactive());
        assert!(!Role::Table.is_interactive());
        assert!(!Role::Unknown.is_interactive());
    }

    #[test]
    fn display_format() {
        assert_eq!(Role::Button.to_string(), "Button");
        assert_eq!(Role::TextField.to_string(), "TextField");
        assert_eq!(Role::StaticText.to_string(), "StaticText");
        assert_eq!(Role::Unknown.to_string(), "Unknown");
    }

    #[test]
    fn short_name() {
        assert_eq!(Role::Button.short_name(), "Button");
        assert_eq!(Role::TextField.short_name(), "TextField");
    }

    #[test]
    fn annotation_category_button() {
        assert_eq!(
            Role::Button.annotation_category(),
            AnnotationCategory::Button
        );
        assert_eq!(
            Role::MenuButton.annotation_category(),
            AnnotationCategory::Button
        );
        assert_eq!(
            Role::DockItem.annotation_category(),
            AnnotationCategory::Button
        );
        assert_eq!(
            Role::Incrementor.annotation_category(),
            AnnotationCategory::Button
        );
    }

    #[test]
    fn annotation_category_text_input() {
        assert_eq!(
            Role::TextField.annotation_category(),
            AnnotationCategory::TextInput
        );
        assert_eq!(
            Role::TextArea.annotation_category(),
            AnnotationCategory::TextInput
        );
        assert_eq!(
            Role::SecureTextField.annotation_category(),
            AnnotationCategory::TextInput
        );
    }

    #[test]
    fn annotation_category_selection() {
        assert_eq!(
            Role::CheckBox.annotation_category(),
            AnnotationCategory::Selection
        );
        assert_eq!(
            Role::RadioButton.annotation_category(),
            AnnotationCategory::Selection
        );
        assert_eq!(
            Role::Slider.annotation_category(),
            AnnotationCategory::Selection
        );
        assert_eq!(
            Role::ComboBox.annotation_category(),
            AnnotationCategory::Selection
        );
        assert_eq!(
            Role::PopUpButton.annotation_category(),
            AnnotationCategory::Selection
        );
        assert_eq!(
            Role::Switch.annotation_category(),
            AnnotationCategory::Selection
        );
        assert_eq!(
            Role::ColorWell.annotation_category(),
            AnnotationCategory::Selection
        );
    }

    #[test]
    fn annotation_category_navigation() {
        assert_eq!(
            Role::Link.annotation_category(),
            AnnotationCategory::Navigation
        );
        assert_eq!(
            Role::Tab.annotation_category(),
            AnnotationCategory::Navigation
        );
        assert_eq!(
            Role::MenuItem.annotation_category(),
            AnnotationCategory::Navigation
        );
        assert_eq!(
            Role::TreeItem.annotation_category(),
            AnnotationCategory::Navigation
        );
    }

    #[test]
    fn annotation_category_other() {
        assert_eq!(Role::Group.annotation_category(), AnnotationCategory::Other);
        assert_eq!(Role::Image.annotation_category(), AnnotationCategory::Other);
        assert_eq!(
            Role::Unknown.annotation_category(),
            AnnotationCategory::Other
        );
    }
}
