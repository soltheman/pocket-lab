param([string]$OutDir)
# Draws the touch buttons' icons: white glyphs with a soft dark edge on transparent
# 128 px squares, one PNG per action.
Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force $OutDir | Out-Null
$S = 128

function New-Canvas {
    $bmp = New-Object System.Drawing.Bitmap $S, $S, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)
    return @($bmp, $g)
}

# Fills `path` white over a dark, slightly larger copy of it (a soft edge on any ground).
function Fill-Glyph($g, $path) {
    $path.FillMode = [System.Drawing.Drawing2D.FillMode]::Winding
    $shadow = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(150, 0, 0, 0)), 7
    $shadow.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
    $g.DrawPath($shadow, $path)
    $g.FillPath([System.Drawing.Brushes]::White, $path)
    $shadow.Dispose()
}

function Stroke-Glyph($g, $path, [float]$width) {
    $shadow = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(150, 0, 0, 0)), ($width + 6)
    $shadow.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
    $shadow.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $shadow.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $g.DrawPath($shadow, $path)
    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), $width
    $pen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $g.DrawPath($pen, $path)
    $shadow.Dispose(); $pen.Dispose()
}

function Save($bmp, $g, $name) {
    $g.Dispose()
    $bmp.Save((Join-Path $OutDir "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

function P([float[]]$xy) {
    $pts = @()
    for ($i = 0; $i -lt $xy.Length; $i += 2) { $pts += New-Object System.Drawing.PointF $xy[$i], $xy[$i + 1] }
    return [System.Drawing.PointF[]]$pts
}

# FIRE: a cartridge, nose up.
$bmp, $g = New-Canvas
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddLine(48, 58, 48, 104); $p.AddLine(48, 104, 80, 104); $p.AddLine(80, 104, 80, 58)
$p.AddBezier(80, 58, 80, 38, 70, 24, 64, 18); $p.AddBezier(64, 18, 58, 24, 48, 38, 48, 58)
$p.CloseFigure()
Fill-Glyph $g $p
$band = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(200, 40, 40, 40))
$g.FillRectangle($band, 48, 62, 32, 5); $g.FillRectangle($band, 44, 104, 40, 8)
$g.FillRectangle([System.Drawing.Brushes]::White, 44, 106, 40, 6)
Save $bmp $g "fire"

# AIM: a scope reticle.
$bmp, $g = New-Canvas
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddEllipse(26, 26, 76, 76)
Stroke-Glyph $g $p 7
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddLine(64, 14, 64, 46); $p.StartFigure(); $p.AddLine(64, 82, 64, 114)
$p.StartFigure(); $p.AddLine(14, 64, 46, 64); $p.StartFigure(); $p.AddLine(82, 64, 114, 64)
Stroke-Glyph $g $p 6
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddEllipse(58, 58, 12, 12)
Fill-Glyph $g $p
Save $bmp $g "aim"

# RELOAD: a circular arrow.
$bmp, $g = New-Canvas
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddArc(28, 28, 72, 72, -60, 290)
Stroke-Glyph $g $p 10
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddPolygon((P @(78, 14, 104, 38, 72, 46)))
Fill-Glyph $g $p
Save $bmp $g "reload"

# USE: an open hand.
$bmp, $g = New-Canvas
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
foreach ($f in @(@(38, 30, 54), @(53, 20, 62), @(68, 22, 60), @(83, 32, 52))) {
    $p.AddArc($f[0], $f[1], 13, 13, 180, 180); $p.AddLine($f[0] + 13, $f[1] + 6, $f[0] + 13, $f[1] + $f[2]); $p.AddLine($f[0] + 13, $f[1] + $f[2], $f[0], $f[1] + $f[2]); $p.CloseFigure()
}
$p.AddArc(36, 62, 62, 52, 0, 180); $p.AddLine(36, 88, 36, 70); $p.AddLine(36, 70, 98, 70); $p.CloseFigure()
$p.AddPolygon((P @(36, 80, 18, 62, 24, 54, 44, 70)))
Fill-Glyph $g $p
Save $bmp $g "use"

# JUMP: two chevrons up.
$bmp, $g = New-Canvas
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddLines((P @(30, 66, 64, 32, 98, 66))); $p.StartFigure(); $p.AddLines((P @(30, 96, 64, 62, 98, 96)))
Stroke-Glyph $g $p 11
Save $bmp $g "jump"

# CROUCH: two chevrons down.
$bmp, $g = New-Canvas
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddLines((P @(30, 32, 64, 66, 98, 32))); $p.StartFigure(); $p.AddLines((P @(30, 62, 64, 96, 98, 62)))
Stroke-Glyph $g $p 11
Save $bmp $g "crouch"

# KNIFE: a combat knife, point up and right.
$bmp, $g = New-Canvas
$state = $g.Save()
$g.TranslateTransform(64, 64); $g.RotateTransform(45)
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddPolygon((P @(-9, -8, -9, -46, -2, -58, 9, -40, 9, -8)))
$p.AddRectangle((New-Object System.Drawing.RectangleF -18, -8, 36, 8))
$p.AddRectangle((New-Object System.Drawing.RectangleF -7, 0, 14, 38))
Fill-Glyph $g $p
$g.Restore($state)
Save $bmp $g "knife"

# NADE: a frag grenade with its lever and pin ring.
$bmp, $g = New-Canvas
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddEllipse(34, 46, 56, 66)
$p.AddRectangle((New-Object System.Drawing.RectangleF 52, 34, 22, 14))
$p.AddPolygon((P @(72, 36, 92, 30, 94, 38, 76, 46)))
Fill-Glyph $g $p
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddEllipse(36, 18, 20, 20)
Stroke-Glyph $g $p 5
$grid = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(160, 30, 30, 30)), 3
$g.DrawLine($grid, 62, 50, 62, 110); $g.DrawLine($grid, 36, 78, 88, 78)
Save $bmp $g "nade"

# SWAP: two arrows, one each way.
$bmp, $g = New-Canvas
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddLine(24, 48, 92, 48); $p.StartFigure(); $p.AddLine(104, 82, 36, 82)
Stroke-Glyph $g $p 9
$p = New-Object System.Drawing.Drawing2D.GraphicsPath
$p.AddPolygon((P @(84, 30, 108, 48, 84, 66)))
$p.AddPolygon((P @(44, 64, 20, 82, 44, 100)))
Fill-Glyph $g $p
Save $bmp $g "swap"
"done"
