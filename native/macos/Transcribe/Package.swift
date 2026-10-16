// swift-tools-version: 5.9
import PackageDescription

// Ajudante nativo do DOTS (copiado do ovseer-transcribe do ovrgit): transcreve um arquivo de áudio em texto, neste Mac.
let package = Package(
    name: "Transcribe",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "dots-transcribe", targets: ["dots-transcribe"]),
    ],
    targets: [
        .executableTarget(name: "dots-transcribe"),
    ]
)
