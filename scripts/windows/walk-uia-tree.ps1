# walk-uia-tree.ps1 -- Walk the UI Automation tree and output element info.
#
# Usage (from macOS via winrun -i):
#   winrun -i scripts/windows/walk-uia-tree.ps1
#   winrun -i scripts/windows/walk-uia-tree.ps1 -AppName Notepad
#
# Outputs element tree with role, name, control type, bounding rectangle, and
# available patterns. Format mimics forepaw's snapshot output for easy comparison.
#
# Must run in interactive session (session 1) -- UIA requires desktop access.
# Use winrun -i to execute via scheduled task.

param(
    [string]$AppName = "",
    [int]$MaxDepth = 8,
    [int]$MaxChildren = 50
)

Add-Type -AssemblyName UIAutomationClient

# Map ControlType enum to short role name (similar to forepaw's AXRole mapping)
function Get-RoleName($controlType) {
    switch ($controlType) {
        { $_ -eq 50000 } { "Button" }
        { $_ -eq 50001 } { "Calendar" }
        { $_ -eq 50002 } { "CheckBox" }
        { $_ -eq 50003 } { "ComboBox" }
        { $_ -eq 50004 } { "Edit" }
        { $_ -eq 50005 } { "Hyperlink" }
        { $_ -eq 50006 } { "Image" }
        { $_ -eq 50007 } { "ListItem" }
        { $_ -eq 50008 } { "List" }
        { $_ -eq 50009 } { "Menu" }
        { $_ -eq 50010 } { "MenuBar" }
        { $_ -eq 50011 } { "MenuItem" }
        { $_ -eq 50012 } { "ProgressBar" }
        { $_ -eq 50013 } { "RadioButton" }
        { $_ -eq 50014 } { "ScrollBar" }
        { $_ -eq 50015 } { "Slider" }
        { $_ -eq 50016 } { "Spinner" }
        { $_ -eq 50017 } { "StatusBar" }
        { $_ -eq 50018 } { "Tab" }
        { $_ -eq 50019 } { "TabItem" }
        { $_ -eq 50020 } { "Text" }
        { $_ -eq 50021 } { "ToolBar" }
        { $_ -eq 50022 } { "ToolTip" }
        { $_ -eq 50023 } { "Tree" }
        { $_ -eq 50024 } { "TreeItem" }
        { $_ -eq 50025 } { "Custom" }
        { $_ -eq 50026 } { "Group" }
        { $_ -eq 50027 } { "Thumb" }
        { $_ -eq 50028 } { "DataGrid" }
        { $_ -eq 50029 } { "DataItem" }
        { $_ -eq 50030 } { "Document" }
        { $_ -eq 50031 } { "SplitButton" }
        { $_ -eq 50032 } { "Window" }
        { $_ -eq 50033 } { "Pane" }
        { $_ -eq 50034 } { "Header" }
        { $_ -eq 50035 } { "HeaderItem" }
        { $_ -eq 50036 } { "Table" }
        { $_ -eq 50037 } { "TitleBar" }
        { $_ -eq 50038 } { "Separator" }
        { $_ -eq 50039 } { "SemanticZoom" }
        { $_ -eq 50040 } { "AppBar" }
        { $_ -eq 50041 } { "Pane" }
        default { "Unknown($_)" }
    }
}

function Get-Patterns($element) {
    $patterns = @()
    # Check common patterns by their IDs
    $patternIds = @{
        10000 = "Invoke"
        10001 = "Selection"
        10002 = "Value"
        10003 = "RangeValue"
        10004 = "Scroll"
        10005 = "ScrollItem"
        10006 = "ExpandCollapse"
        10007 = "Grid"
        10008 = "GridItem"
        10009 = "MultipleView"
        10010 = "Window"
        10011 = "SelectionItem"
        10012 = "Dock"
        10013 = "Table"
        10014 = "TableItem"
        10015 = "Text"
        10016 = "Toggle"
        10017 = "Transform"
        10018 = "ScrollItem"
        10019 = "ItemContainer"
        10020 = "VirtualizedItem"
        10021 = "SynchronizedInput"
        10022 = "ObjectModel"
        10023 = "Annotation"
        10024 = "Text2"
        10025 = "Styles"
        10026 = "Spreadsheet"
        10027 = "SpreadsheetItem"
        10028 = "Transform2"
        10029 = "TextChild"
        10030 = "Drag"
        10031 = "Drop"
        10032 = "TextEdit"
        10033 = "CustomNavigation"
        10034 = "Spreadsheet"
    }

    foreach ($id in $patternIds.Keys) {
        try {
            $pattern = $element.GetCurrentPattern($id)
            if ($pattern -ne $null) {
                $patterns += $patternIds[$id]
            }
        } catch {
            # Pattern not available, skip
        }
    }
    return $patterns
}

$refCounter = 0

function Walk-Tree($element, $depth, $prefix) {
    if ($depth -gt $MaxDepth) { return }
    $script:refCounter++
    $ref = "e$script:refCounter"

    $name = ""
    try { $name = $element.Current.Name } catch {}
    $controlType = 0
    try { $controlType = $element.Current.ControlType } catch {}
    $role = Get-RoleName $controlType
    $rect = [System.Windows.Rect]::Empty
    try { $rect = $element.Current.BoundingRectangle } catch {}

    # Build bounding box string
    if ($rect.IsEmpty -or ($rect.Width -eq 0 -and $rect.Height -eq 0)) {
        $bounds = "bounds={}"
    } else {
        $bounds = "bounds={$( [int]$rect.X ),$( [int]$rect.Y ),$( [int]$rect.Width ),$( [int]$rect.Height )}"
    }

    # Get patterns (only for interactive-looking elements)
    $patternStr = ""
    if ($controlType -in @(50000, 50002, 50003, 50004, 50010, 50011, 50016, 50018, 50019, 50032)) {
        $pats = Get-Patterns $element
        if ($pats.Count -gt 0) {
            $patternStr = " patterns=" + ($pats -join ",")
        }
    }

    # Truncate long names
    if ($name.Length -gt 80) { $name = $name.Substring(0, 77) + "..." }

    $indent = "  " * $depth
    Write-Host "${indent}@${ref} ${role} `"$name`" ${bounds}${patternStr}"

    # Get children
    try {
        $cond = [System.Windows.Automation.Condition]::TrueCondition
        $children = $element.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)
        $count = [Math]::Min($children.Count, $MaxChildren)
        for ($i = 0; $i -lt $count; $i++) {
            Walk-Tree $children[$i] ($depth + 1) ""
        }
        if ($children.Count -gt $MaxChildren) {
            $indent2 = "  " * ($depth + 1)
            Write-Host "${indent2}... ($($children.Count - $MaxChildren) more children)"
        }
    } catch {}
}

# Find target window
$root = [System.Windows.Automation.AutomationElement]::RootElement

if ($AppName -ne "") {
    Write-Host "Searching for window: $AppName"
    $cond = [System.Windows.Automation.Condition]::TrueCondition
    $children = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)
    $found = $null
    foreach ($child in $children) {
        $n = ""
        try { $n = $child.Current.Name } catch {}
        if ($n -like "*$AppName*") {
            $found = $child
            break
        }
    }
    if ($found) {
        $name = ""
        try { $name = $found.Current.Name } catch {}
        Write-Host "Found: $name"
        Write-Host "---"
        Walk-Tree $found 0 ""
    } else {
        Write-Host "Window not found: $AppName"
        Write-Host "Available windows:"
        foreach ($child in $children) {
            $n = ""
            try { $n = $child.Current.Name } catch {}
            $ct = 0
            try { $ct = $child.Current.ControlType } catch {}
            Write-Host "  [$(Get-RoleName $ct)] $n"
        }
    }
} else {
    Write-Host "=== Desktop (all top-level windows) ==="
    Walk-Tree $root 0 ""
}

Write-Host "---"
Write-Host "Total elements: $script:refCounter"
