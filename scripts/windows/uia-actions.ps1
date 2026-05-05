# uia-actions.ps1 -- Perform actions via UI Automation patterns.
#
# Usage (from macOS via winrun -i):
#   # Click a button by name in a window
#   winrun -i scripts/windows/uia-actions.ps1 -Action Click -WindowName Notepad -ElementName "File"
#   # Set text value
#   winrun -i scripts/windows/uia-actions.ps1 -Action Type -WindowName Notepad -ElementName "Text Editor" -Value "Hello World"
#   # List available actions for an element
#   winrun -i scripts/windows/uia-actions.ps1 -Action Info -WindowName Notepad -ElementName "File"
#
# Must run in interactive session (session 1) via winrun -i.

param(
    [Parameter(Mandatory=$true)]
    [ValidateSet("Click", "Type", "Toggle", "Expand", "Collapse", "Focus", "Info")]
    [string]$Action,

    [Parameter(Mandatory=$true)]
    [string]$WindowName,

    [Parameter(Mandatory=$true)]
    [string]$ElementName,

    [string]$Value = ""
)

Add-Type -AssemblyName UIAutomationClient

# Find the target window
$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = [System.Windows.Automation.Condition]::TrueCondition
$children = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)
$window = $null
foreach ($child in $children) {
    $n = ""
    try { $n = $child.Current.Name } catch {}
    if ($n -like "*$WindowName*") {
        $window = $child
        break
    }
}
if (-not $window) {
    Write-Host "ERROR: Window not found: $WindowName"
    Write-Host "Available windows:"
    foreach ($child in $children) {
        $n = ""
        try { $n = $child.Current.Name } catch {}
        Write-Host "  $n"
    }
    exit 1
}

# Search for the element within the window (depth-first, up to 500 elements)
function Find-Element($parent, $name, [ref]$count) {
    if ($count.Value -gt 500) { return $null }
    $cond = [System.Windows.Automation.Condition]::TrueCondition
    $kids = $parent.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)
    foreach ($kid in $kids) {
        $count.Value++
        $n = ""
        try { $n = $kid.Current.Name } catch {}
        if ($n -like "*$name*") { return $kid }
        $found = Find-Element $kid $name $count
        if ($found) { return $found }
    }
    return $null
}

$countRef = [ref]0
$element = Find-Element $window $ElementName $countRef
if (-not $element) {
    Write-Host "ERROR: Element not found: $ElementName (searched $($countRef.Value) elements)"
    exit 1
}

$elemName = ""
try { $elemName = $element.Current.Name } catch {}
Write-Host "Found element: `"$elemName`" (searched $($countRef.Value) elements)"

switch ($Action) {
    "Click" {
        try {
            $pattern = $element.GetCurrentPattern(10000) # InvokePattern
            if ($pattern) {
                $pattern.Invoke()
                Write-Host "OK: Invoked (clicked)"
            } else {
                # Fallback: get bounding rect and click center
                $rect = $element.Current.BoundingRectangle
                $cx = [int]($rect.X + $rect.Width / 2)
                $cy = [int]($rect.Y + $rect.Height / 2)
                Write-Host "No InvokePattern. Bounding rect center: $cx, $cy"
                Write-Host "Use SendInput mouse click at those coordinates."
            }
        } catch {
            Write-Host "ERROR: $($_.Exception.Message)"
        }
    }
    "Type" {
        if ($Value -eq "") {
            Write-Host "ERROR: -Value is required for Type action"
            exit 1
        }
        try {
            $pattern = $element.GetCurrentPattern(10002) # ValuePattern
            if ($pattern) {
                $pattern.SetValue($Value)
                Write-Host "OK: Set value to `"$Value`""
            } else {
                Write-Host "ERROR: Element does not support ValuePattern"
            }
        } catch {
            Write-Host "ERROR: $($_.Exception.Message)"
        }
    }
    "Toggle" {
        try {
            $pattern = $element.GetCurrentPattern(10016) # TogglePattern
            if ($pattern) {
                $pattern.Toggle()
                Write-Host "OK: Toggled"
            } else {
                Write-Host "ERROR: Element does not support TogglePattern"
            }
        } catch {
            Write-Host "ERROR: $($_.Exception.Message)"
        }
    }
    "Expand" {
        try {
            $pattern = $element.GetCurrentPattern(10006) # ExpandCollapsePattern
            if ($pattern) {
                $pattern.Expand()
                Write-Host "OK: Expanded"
            } else {
                Write-Host "ERROR: Element does not support ExpandCollapsePattern"
            }
        } catch {
            Write-Host "ERROR: $($_.Exception.Message)"
        }
    }
    "Collapse" {
        try {
            $pattern = $element.GetCurrentPattern(10006) # ExpandCollapsePattern
            if ($pattern) {
                $pattern.Collapse()
                Write-Host "OK: Collapsed"
            } else {
                Write-Host "ERROR: Element does not support ExpandCollapsePattern"
            }
        } catch {
            Write-Host "ERROR: $($_.Exception.Message)"
        }
    }
    "Focus" {
        try {
            $element.SetFocus()
            Write-Host "OK: Focused"
        } catch {
            Write-Host "ERROR: $($_.Exception.Message)"
        }
    }
    "Info" {
        $ct = 0
        try { $ct = $element.Current.ControlType } catch {}
        $rect = [System.Windows.Rect]::Empty
        try { $rect = $element.Current.BoundingRectangle } catch {}
        $autoId = ""
        try { $autoId = $element.Current.AutomationId } catch {}
        $className = ""
        try { $className = $element.Current.ClassName } catch {}
        $isEnabled = $true
        try { $isEnabled = $element.Current.IsEnabled } catch {}
        $isOffscreen = $false
        try { $isOffscreen = $element.Current.IsOffscreen } catch {}

        Write-Host "Name: $elemName"
        Write-Host "ControlType: $ct"
        Write-Host "AutomationId: $autoId"
        Write-Host "ClassName: $className"
        Write-Host "Enabled: $isEnabled"
        Write-Host "Offscreen: $isOffscreen"
        if ($rect.IsEmpty) {
            Write-Host "Bounds: (empty)"
        } else {
            Write-Host "Bounds: $( [int]$rect.X ),$( [int]$rect.Y ),$( [int]$rect.Width ),$( [int]$rect.Height )"
        }

        # List available patterns
        $patternIds = @{
            10000 = "Invoke"; 10002 = "Value"; 10004 = "Scroll"
            10006 = "ExpandCollapse"; 10010 = "Window"; 10011 = "SelectionItem"
            10015 = "Text"; 10016 = "Toggle"; 10017 = "Transform"
        }
        $available = @()
        foreach ($id in $patternIds.Keys) {
            try {
                $p = $element.GetCurrentPattern($id)
                if ($p) { $available += $patternIds[$id] }
            } catch {}
        }
        Write-Host "Patterns: $($available -join ', ')"
    }
}
