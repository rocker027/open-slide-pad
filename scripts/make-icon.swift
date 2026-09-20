import AppKit

// 可重建的原生向量圖示；不使用外部品牌資產。
let size = NSSize(width: 1024, height: 1024)
let icon = NSImage(size: size)
icon.lockFocus()
NSColor(calibratedRed: 0.07, green: 0.095, blue: 0.075, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 28, y: 28, width: 968, height: 968), xRadius: 215, yRadius: 215).fill()
let shadow = NSShadow()
shadow.shadowColor = NSColor.black.withAlphaComponent(0.3)
shadow.shadowBlurRadius = 32
shadow.shadowOffset = NSSize(width: 0, height: -14)
shadow.set()
NSColor(calibratedRed: 0.24, green: 0.32, blue: 0.21, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 206, y: 241, width: 425, height: 572), xRadius: 62, yRadius: 62).fill()
NSColor(calibratedRed: 0.765, green: 0.937, blue: 0.643, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 348, y: 178, width: 468, height: 585), xRadius: 64, yRadius: 64).fill()
NSShadow().set()
NSColor(calibratedRed: 0.15, green: 0.24, blue: 0.12, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 412, y: 254, width: 75, height: 433), xRadius: 18, yRadius: 18).fill()
NSBezierPath(roundedRect: NSRect(x: 523, y: 606, width: 216, height: 36), xRadius: 12, yRadius: 12).fill()
NSColor(calibratedRed: 0.47, green: 0.65, blue: 0.36, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 523, y: 530, width: 155, height: 27), xRadius: 10, yRadius: 10).fill()
icon.unlockFocus()
guard let tiff = icon.tiffRepresentation, let bitmap = NSBitmapImageRep(data: tiff), let png = bitmap.representation(using: .png, properties: [:]) else { fatalError("無法產生圖示") }
try png.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
