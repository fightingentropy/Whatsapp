import SwiftUI
import UniformTypeIdentifiers

struct MediaPicker: View {
    enum Tab: String, Identifiable { case emoji, gifs, stickers; var id: String { rawValue } }
    var insertEmoji: (String) -> Void
    @Environment(ChatStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var tab: Tab
    @State private var query = ""
    @State private var packURL = ""
    @State private var importPresented = false
    @State private var deletePack: StickerPack?
    init(initialTab: Tab = .emoji, insertEmoji: @escaping (String) -> Void) {
        self.insertEmoji = insertEmoji
        _tab = State(initialValue: initialTab)
    }
    var body: some View {
        NavigationStack {
            VStack {
                Picker("Picker", selection: $tab) {
                    Text("Emoji").tag(Tab.emoji); Text("GIFs").tag(Tab.gifs); Text("Stickers").tag(Tab.stickers)
                }.pickerStyle(.segmented).padding(.horizontal)
                ScrollView {
                    if tab == .emoji { emojiGrid }
                    else if tab == .gifs { gifGrid }
                    else { stickerGrid }
                }
            }
            .navigationTitle("Add to your message").navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
            .searchable(text: $query, prompt: tab == .gifs ? "Search GIPHY" : "Search")
            .task(id: tab.rawValue + query) {
                if tab == .gifs {
                    do { try await Task.sleep(for: .milliseconds(250)) } catch { return }
                    store.searchGifs(query)
                } else if tab == .stickers { store.loadStickers() }
            }
            .fileImporter(isPresented: $importPresented, allowedContentTypes: [.zip, UTType(filenameExtension: "wastickers") ?? .data]) { result in
                guard case .success(let url) = result else { return }
                Task {
                    do { let file = try await Task.detached { try AttachmentImport.copy(url) }.value; store.importStickers(file: file) }
                    catch { store.error = "Could not open this sticker pack." }
                }
            }
            .confirmationDialog("Delete this saved sticker pack?", isPresented: Binding(get: { deletePack != nil }, set: { if !$0 { deletePack = nil } }), titleVisibility: .visible) {
                if let pack = deletePack { Button("Delete \(pack.name)", role: .destructive) { store.deletePack(pack); deletePack = nil } }
            }
        }
    }

    private var emojiGrid: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible()), count: 7), spacing: 18) {
            ForEach(EmojiCatalog.search(query, recent: store.preferences.recentEmoji)) { entry in
                Button {
                    store.rememberEmoji(entry.emoji)
                    insertEmoji(entry.emoji)
                } label: { Text(entry.emoji).font(.system(size: 31)) }.accessibilityLabel(entry.name)
            }
        }.padding()
    }

    private var gifGrid: some View {
        VStack {
            if store.preferences.giphyKey.isEmpty {
                ContentUnavailableView("Connect GIPHY", systemImage: "photo.on.rectangle", description: Text("Add your GIPHY API key in Settings to search and send GIFs."))
            } else if store.gifsLoading { ProgressView("Searching GIPHY…").padding() }
            if let error = store.gifError { Text(error).font(.footnote).foregroundStyle(.secondary).padding() }
            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())]) {
                ForEach(store.gifs) { gif in
                    Button { store.sendGif(gif); dismiss() } label: {
                        if let url = store.localURL(gif.still) { LocalImage(url: url, maximumSize: 360).scaledToFit().frame(height: 130) }
                        else { Image(systemName: "photo").frame(height: 130) }
                    }.accessibilityLabel("Send GIF").disabled(!store.canPost)
                }
            }.padding()
            Text("Powered by GIPHY").font(.caption).foregroundStyle(.secondary)
        }
    }

    private var stickerGrid: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack {
                TextField("signal.art pack link", text: $packURL).textInputAutocapitalization(.never).autocorrectionDisabled()
                Button("Import") { store.importStickers(url: packURL); packURL = "" }.disabled(packURL.isEmpty)
            }.textFieldStyle(.roundedBorder)
            Button("Import a .wastickers or ZIP file") { importPresented = true }
            if !store.savedStickers.isEmpty { Text("Saved").font(.headline); stickers(store.savedStickers, saved: true) }
            if !store.recentStickers.isEmpty { Text("Recent from your phone").font(.headline); stickers(store.recentStickers, saved: false) }
            ForEach(store.stickerPacks.filter { query.isEmpty || $0.name.localizedStandardContains(query) }) { pack in
                HStack { Text(pack.name).font(.headline); Spacer(); Button { deletePack = pack } label: { Image(systemName: "trash") }.tint(.secondary) }
                stickers(pack.stickers, saved: false)
            }
            if store.savedStickers.isEmpty && store.recentStickers.isEmpty && store.stickerPacks.isEmpty {
                Text("Save a sticker from a message, or import a pack above.").foregroundStyle(.secondary)
            }
        }.padding()
    }

    private func stickers(_ paths: [String], saved: Bool) -> some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible()), count: 3)) {
            ForEach(paths, id: \.self) { path in
                if let url = store.localURL(path) {
                    Button { store.sendSticker(path); dismiss() } label: {
                        AnimatedMedia(url: url).frame(height: 90)
                    }.disabled(!store.canPost).accessibilityLabel("Send sticker")
                        .contextMenu { Button(saved ? "Remove from saved" : "Save sticker") { store.saveSticker(path, remove: saved) } }
                }
            }
        }
    }
}
