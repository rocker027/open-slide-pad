import AppKit

// 保留設計原稿；只在封裝時產生 macOS 所需的各尺寸圖示。
let source = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "resources/AppIcon.png"
let destination = CommandLine.arguments.count > 2 ? CommandLine.arguments[2] : "resources/AppIcon.icns"
guard let icon = NSImage(contentsOfFile: source) else {
    fatalError("無法讀取圖示原稿：\(source)")
}
let staging = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
let iconset = staging.appendingPathComponent("AppIcon.iconset")
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(at: staging) }

func writeRepresentation(points: Int, scale: Int) throws {
    let pixels = points * scale
    guard let bitmap = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true,
        isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
    ), let context = NSGraphicsContext(bitmapImageRep: bitmap) else {
        fatalError("無法建立圖示畫布")
    }
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = context
    context.imageInterpolation = .high
    icon.draw(in: NSRect(x: 0, y: 0, width: pixels, height: pixels))
    NSGraphicsContext.restoreGraphicsState()
    guard let png = bitmap.representation(using: .png, properties: [:]) else {
        fatalError("無法編碼圖示")
    }
    let suffix = scale == 2 ? "@2x" : ""
    try png.write(to: iconset.appendingPathComponent("icon_\(points)x\(points)\(suffix).png"))
}

for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] { try writeRepresentation(points: points, scale: scale) }
}
let conversion = Process()
conversion.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
conversion.arguments = ["-c", "icns", iconset.path, "-o", destination]
try conversion.run()
conversion.waitUntilExit()
guard conversion.terminationStatus == 0 else { fatalError("icns 封裝失敗") }
print("已產生 \(destination)")
