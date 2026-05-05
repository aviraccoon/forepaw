# ocr.ps1 -- Run Windows.Media.Ocr on an image file.
#
# Usage (from macOS via winrun -i):
#   winrun -i scripts/windows/ocr.ps1
#   winrun -i scripts/windows/ocr.ps1 -Path C:\Users\user\test.png
#   winrun -i scripts/windows/ocr.ps1 -Language en-US
#
# Default path is C:\Users\user\screenshot.png (output of screenshot.ps1).
# Outputs each line with bounding box coordinates and word-level detail.
#
# IMPORTANT: Must run via Windows PowerShell 5.1 (not PS7) -- WinRT type loading
# only works in Windows PowerShell. Must run in interactive session (winrun -i).
#
# Windows.Media.Ocr requires no MSIX packaging, no special identity -- it works
# from any desktop process in the interactive session.

param(
    [string]$Path = "C:\Users\user\screenshot.png",
    [string]$Language = ""
)

# Load WinRT types (only works in Windows PowerShell 5.1)
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$null = [Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]
$null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Foundation.IAsyncOperation`1, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Graphics.Imaging.SoftwareBitmap, Windows.Foundation, ContentType = WindowsRuntime]
$null = [Windows.Storage.Streams.RandomAccessStream, Windows.Storage.Streams, ContentType = WindowsRuntime]
$null = [WindowsRuntimeSystemExtensions]
$null = [Windows.Media.Ocr.OcrEngine]::AvailableRecognizerLanguages

# Await helper for WinRT async methods
$awaiter = [WindowsRuntimeSystemExtensions].GetMember('GetAwaiter', 'Method', 'Public,Static') |
    Where-Object { $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' } |
    Select-Object -First 1

function Invoke-Async([object]$AsyncTask, [Type]$As) {
    return $awaiter.MakeGenericMethod($As).Invoke($null, @($AsyncTask)).GetResult()
}

# Create OCR engine
if ($Language -ne "") {
    $lang = New-Object Windows.Globalization.Language($Language)
    $ocrEngine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromLanguage($lang)
} else {
    $ocrEngine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
}

if (-not $ocrEngine) {
    Write-Host "ERROR: No OCR engine available. Install a language pack?"
    exit 1
}

# Check file exists
if (-not (Test-Path $Path)) {
    Write-Host "ERROR: File not found: $Path"
    exit 1
}

# Open and decode the image
$file = [Windows.Storage.StorageFile]::GetFileFromPathAsync($Path)
$storageFile = Invoke-Async $file -As ([Windows.Storage.StorageFile])
$content = $storageFile.OpenAsync([Windows.Storage.FileAccessMode]::Read)
$fileStream = Invoke-Async $content -As ([Windows.Storage.Streams.IRandomAccessStream])
$decoder = [Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($fileStream)
$bitmapDecoder = Invoke-Async $decoder -As ([Windows.Graphics.Imaging.BitmapDecoder])
$bitmap = $bitmapDecoder.GetSoftwareBitmapAsync()
$softwareBitmap = Invoke-Async $bitmap -As ([Windows.Graphics.Imaging.SoftwareBitmap])

# Run OCR
$ocrResult = Invoke-Async $ocrEngine.RecognizeAsync($softwareBitmap) -As ([Windows.Media.Ocr.OcrResult])

Write-Host "OCR: $(@($ocrResult.Lines).Count) lines from $Path"
Write-Host "---"
foreach ($line in $ocrResult.Lines) {
    $words = @()
    foreach ($word in $line.Words) {
        $r = $word.BoundingRect
        $words += "`"$($word.Text)`" @{$([int]$r.X),$([int]$r.Y),$([int]$r.Width),$([int]$r.Height)}"
    }
    Write-Host "$($line.Text)"
    if (@($line.Words).Count -le 20) {
        Write-Host "  words: $($words -join ' ')"
    }
}
