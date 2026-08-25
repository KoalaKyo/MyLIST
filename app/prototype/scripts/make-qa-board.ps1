param(
  [Parameter(Mandatory = $true)][string]$ReferencePath,
  [Parameter(Mandatory = $true)][string]$ImplementationPath,
  [Parameter(Mandatory = $true)][string]$OutputPath
)

Add-Type -AssemblyName System.Drawing

$reference = [System.Drawing.Image]::FromFile($ReferencePath)
$implementation = [System.Drawing.Image]::FromFile($ImplementationPath)
$canvas = $null
$graphics = $null
$titleFont = $null
$metaFont = $null
$labelBrush = $null
$metaBrush = $null
$panelBrush = $null

try {
  $targetHeight = 520
  $referenceWidth = [int][Math]::Round($reference.Width * $targetHeight / $reference.Height)
  $implementationWidth = [int][Math]::Round($implementation.Width * $targetHeight / $implementation.Height)
  $outer = 24
  $gap = 24
  $header = 64
  $footer = 34
  $canvasWidth = $outer * 2 + $referenceWidth + $implementationWidth + $gap
  $canvasHeight = $header + $targetHeight + $footer

  $canvas = New-Object System.Drawing.Bitmap($canvasWidth, $canvasHeight)
  $graphics = [System.Drawing.Graphics]::FromImage($canvas)
  $graphics.Clear([System.Drawing.Color]::FromArgb(242, 246, 251))
  $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
  $graphics.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::ClearTypeGridFit

  $titleFont = New-Object System.Drawing.Font('Segoe UI', 12, [System.Drawing.FontStyle]::Bold)
  $metaFont = New-Object System.Drawing.Font('Segoe UI', 9, [System.Drawing.FontStyle]::Regular)
  $labelBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(31, 43, 61))
  $metaBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(99, 113, 134))
  $panelBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)

  $leftX = $outer
  $rightX = $outer + $referenceWidth + $gap
  $imageY = $header

  $graphics.FillRectangle($panelBrush, $leftX, $imageY, $referenceWidth, $targetHeight)
  $graphics.FillRectangle($panelBrush, $rightX, $imageY, $implementationWidth, $targetHeight)
  $graphics.DrawImage($reference, $leftX, $imageY, $referenceWidth, $targetHeight)
  $graphics.DrawImage($implementation, $rightX, $imageY, $implementationWidth, $targetHeight)
  $graphics.DrawString('VISUAL REFERENCE', $titleFont, $labelBrush, $leftX, 16)
  $graphics.DrawString('HTML IMPLEMENTATION', $titleFont, $labelBrush, $rightX, 16)
  $graphics.DrawString('Source: concept-1.png', $metaFont, $metaBrush, $leftX, 39)
  $graphics.DrawString('State: light theme / todo list', $metaFont, $metaBrush, $rightX, 39)
  $graphics.DrawString('QA: layout, density, hierarchy, color, iconography, and interaction entry points', $metaFont, $metaBrush, $outer, $header + $targetHeight + 9)

  $outputDirectory = [System.IO.Path]::GetDirectoryName($OutputPath)
  [System.IO.Directory]::CreateDirectory($outputDirectory) | Out-Null
  $canvas.Save($OutputPath, [System.Drawing.Imaging.ImageFormat]::Png)
}
finally {
  if ($panelBrush) { $panelBrush.Dispose() }
  if ($metaBrush) { $metaBrush.Dispose() }
  if ($labelBrush) { $labelBrush.Dispose() }
  if ($metaFont) { $metaFont.Dispose() }
  if ($titleFont) { $titleFont.Dispose() }
  if ($graphics) { $graphics.Dispose() }
  if ($canvas) { $canvas.Dispose() }
  if ($implementation) { $implementation.Dispose() }
  if ($reference) { $reference.Dispose() }
}
