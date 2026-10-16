import AVFoundation
import Foundation
import Speech

// dots-transcribe: transforma um arquivo de áudio em texto (copiado do dots-transcribe do ovrgit).
//
//   dots-transcribe --input <arquivo de áudio> --locale <id BCP 47>
//
// O reconhecimento roda neste Mac (SpeechAnalyzer); o áudio não sai dele.
// O andamento e o resultado saem no stdout, uma linha JSON por vez:
//
//   {"type":"status","state":"preparing" | "downloading" | "transcribing"}
//   {"type":"text","text":"Olá, tudo bem?"}
//   {"type":"done"}
//   {"type":"error","code":"unsupported-os" | "unsupported-locale" | "unreadable-audio" | "failed","detail":"…"}
//
// Cancelar é encerrar o processo.

struct Message: Encodable {
    let type: String
    var state: String?
    var text: String?
    var code: String?
    var detail: String?
}

func emit(_ message: Message) {
    guard var line = try? JSONEncoder().encode(message) else { return }
    line.append(0x0A)
    FileHandle.standardOutput.write(line)
}

func fail(_ code: String, _ detail: String) -> Never {
    emit(Message(type: "error", code: code, detail: detail))
    exit(1)
}

func argument(_ name: String) -> String? {
    let arguments = CommandLine.arguments
    guard let index = arguments.firstIndex(of: name), index + 1 < arguments.count else { return nil }
    return arguments[index + 1]
}

@available(macOS 26.0, *)
func transcribe(input: URL, localeId: String) async {
    emit(Message(type: "status", state: "preparing"))

    let wanted = Locale(identifier: localeId).identifier(.bcp47)
    let supported = await SpeechTranscriber.supportedLocales
    guard let locale = supported.first(where: { $0.identifier(.bcp47) == wanted }) else {
        fail("unsupported-locale", wanted)
    }

    let file: AVAudioFile
    do {
        file = try AVAudioFile(forReading: input)
    } catch {
        fail("unreadable-audio", String(describing: error))
    }

    let transcriber = SpeechTranscriber(
        locale: locale,
        transcriptionOptions: [],
        reportingOptions: [],
        attributeOptions: [])

    do {
        // O modelo do idioma é baixado pelo sistema na primeira vez em que o idioma é usado.
        if await AssetInventory.status(forModules: [transcriber]) != .installed {
            emit(Message(type: "status", state: "downloading"))
        }
        if let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
            try await request.downloadAndInstall()
        }

        emit(Message(type: "status", state: "transcribing"))
        let analyzer = SpeechAnalyzer(modules: [transcriber])
        let results = Task {
            for try await result in transcriber.results {
                let text = String(result.text.characters)
                if !text.isEmpty { emit(Message(type: "text", text: text)) }
            }
        }

        if let lastSample = try await analyzer.analyzeSequence(from: file) {
            try await analyzer.finalizeAndFinish(through: lastSample)
        } else {
            await analyzer.cancelAndFinishNow()
        }
        try await results.value
        emit(Message(type: "done"))
    } catch {
        fail("failed", String(describing: error))
    }
}

guard let inputPath = argument("--input"), let localeId = argument("--locale") else {
    fail("failed", "uso: dots-transcribe --input <arquivo de áudio> --locale <id>")
}

if #available(macOS 26.0, *) {
    await transcribe(input: URL(fileURLWithPath: inputPath), localeId: localeId)
} else {
    fail("unsupported-os", "a transcrição precisa do macOS 26 ou mais novo")
}
