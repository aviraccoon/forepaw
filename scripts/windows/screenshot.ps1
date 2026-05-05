# screenshot.ps1 -- Capture a screenshot from the interactive session.
#
# Usage (from macOS via winrun -i):
#   winrun -i scripts/windows/screenshot.ps1
#   winrun -i scripts/windows/screenshot.ps1 -WindowName Notepad
#   winrun -i scripts/windows/screenshot.ps1 -OutPath C:\Users\user\test.png
#
# Captures fullscreen or a specific window. Outputs the file path and size.
# The file can be retrieved via scp:
#   scp user@VM:C:/Users/user/screenshot.png /tmp/win-screenshot.png
#
# Must run in interactive session (session 1) via winrun -i.

param(
    [string]$WindowName = "",
    [string]$OutPath = "C:\Users\user\screenshot.png"
)

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

if ($WindowName -ne "") {
    # Capture a specific window by finding it via UIA
    Add-Type -AssemblyName UIAutomationClient
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $cond = [System.Windows.Automation.Condition]::TrueCondition
    $children = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $cond)
    $found = $null
    foreach ($child in $children) {
        $n = ""
        try { $n = $child.Current.Name } catch {}
        if ($n -like "*$WindowName*") {
            $found = $child
            break
        }
    }
    if (-not $found) {
        Write-Host "Window not found: $WindowName"
        exit 1
    }
    $rect = $found.Current.BoundingRectangle
    if ($rect.IsEmpty) {
        Write-Host "Window has no bounding rectangle"
        exit 1
    }

    $x = [int]$rect.X
    $y = [int]$rect.Y
    $w = [int]$rect.Width
    $h = [int]$rect.Height

    Write-Host "Capturing window '$($found.Current.Name)' at ${x},${y} ${w}x${h}"
    $bitmap = New-Object System.Drawing.Bitmap($w, $h)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($x, $y, 0, 0, [System.Drawing.Size]::new($w, $h))
    $graphics.Dispose()
} else {
    # Capture fullscreen
    $screen = [System.Windows.Forms.Screen]::PrimaryScreen
    $bounds = $screen.Bounds
    Write-Host "Capturing screen ${bounds.Width}x${bounds.Height} at ${bounds.X},${bounds.Y}"
    $bitmap = New-Object System.Drawing.Bitmap($bounds.Width, $bounds.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    $graphics.Dispose()
}

$bitmap.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)
$bitmap.Dispose()
$size = (Get-Item $OutPath).Length
Write-Host "Saved: $OutPath ($([math]::Round($size / 1KB, 1)) KB)"
