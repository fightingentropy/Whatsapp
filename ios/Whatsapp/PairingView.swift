import CoreImage.CIFilterBuiltins
import SwiftUI

struct PairingView: View {
    @Environment(ChatStore.self) private var store
    @State private var phone = ""
    @State private var showQR = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 28) {
                    VStack(alignment: .leading, spacing: 14) {
                        HStack(alignment: .center) {
                            Wordmark(text: "whatsapp")
                            Spacer()
                            Image("Brand").resizable().frame(width: 44, height: 44).clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
                        }
                        Text("Your chats, on your iPhone.").font(Theme.sans(22, .semibold)).foregroundStyle(Theme.fg)
                        Text("Link your existing WhatsApp account to this independent companion.")
                            .font(Theme.sans(15)).foregroundStyle(Theme.muted)
                    }
                    if let code = store.pairingCode {
                        Card(padding: 20) { VStack(alignment: .leading, spacing: 18) {
                            Eyebrow("Enter this code in WhatsApp")
                            Text(code).font(Theme.mono(32, .semibold)).foregroundStyle(Theme.fg)
                                .tracking(3).textSelection(.enabled).accessibilityIdentifier("pairing-code")
                            Button {
                                UIPasteboard.general.setItems([[UIPasteboard.typeAutomatic: code]], options: [
                                    .localOnly: true, .expirationDate: Date().addingTimeInterval(120)
                                ])
                            } label: {
                                Label("Copy code", systemImage: "doc.on.doc")
                            }.buttonStyle(PillStyle())
                            Text("Switch to official WhatsApp, enter the code and approve. Return here straight after submitting it so this app can finish connecting.")
                                .font(Theme.sans(14)).foregroundStyle(Theme.muted)
                            Text("iOS gives linking limited time while you switch apps. This code stops working if that time runs out.")
                                .font(Theme.sans(13)).foregroundStyle(Theme.muted)
                        } }
                    } else {
                        Card(padding: 20) { VStack(alignment: .leading, spacing: 14) {
                            if store.pairingInterrupted {
                                HStack(spacing: 6) { Dot(color: Theme.amber); Eyebrow("Linking was interrupted", color: Theme.amber) }
                                Text("This app ran out of background time, so the previous code is no longer active. Request a fresh code and return here straight after submitting it in WhatsApp.")
                                    .font(Theme.sans(14)).foregroundStyle(Theme.muted)
                                    .accessibilityIdentifier("pairing-interrupted")
                            }
                            Eyebrow("Link with phone number")
                            Text("First, open official WhatsApp → Settings → Linked Devices → Link a Device → Link with phone number instead. Then return here to get your code.")
                                .font(Theme.sans(14)).foregroundStyle(Theme.muted)
                            TextField("Country code + phone number", text: $phone)
                                .keyboardType(.phonePad).textContentType(.telephoneNumber)
                                .font(Theme.mono(17)).foregroundStyle(Theme.fg)
                                .padding(.horizontal, 14).frame(height: 50)
                                .background(Theme.bg, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                                .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Theme.line, lineWidth: 1))
                                .accessibilityIdentifier("pairing-phone")
                            Button { store.pair(phone: phone) } label: {
                                HStack {
                                    if store.pairingBusy { ProgressView().tint(Theme.bg) }
                                    Text(store.pairingBusy ? "Getting your code…" : "Link with phone number")
                                }
                            }
                            .buttonStyle(PillStyle(large: true))
                            .disabled(store.pairingBusy || store.status != "unlinked")
                            .accessibilityIdentifier("pairing-submit")
                            Text("Include the country code, for example +44. This adds a linked device; your existing account stays on your phone.")
                                .font(Theme.sans(13)).foregroundStyle(Theme.muted)
                        } }
                    }
                    if store.status == "starting" || store.status == "connecting" {
                        HStack(spacing: 8) { Dot(color: Theme.amber); Text("Connecting securely…").font(Theme.mono(13)).foregroundStyle(Theme.muted) }
                    } else if store.status == "failed" || store.status == "disconnected" || store.status == "logged_out" {
                        Button("Try connecting again") { store.reconnect() }.buttonStyle(PillStyle(kind: .secondary))
                    }
                    if let qr = store.qr {
                        DisclosureGroup("Link using a QR code", isExpanded: $showQR) {
                            VStack(spacing: 14) {
                                if let image = qrImage(qr) {
                                    Image(uiImage: image).interpolation(.none).resizable().scaledToFit()
                                        .frame(width: 220, height: 220).padding(16).background(.white, in: RoundedRectangle(cornerRadius: 16))
                                        .accessibilityLabel("WhatsApp linking QR code")
                                }
                                Text("Use the QR code when your primary WhatsApp account is on another phone.")
                                    .font(Theme.sans(13)).foregroundStyle(Theme.muted)
                            }.frame(maxWidth: .infinity).padding(.top, 14)
                        }.tint(Theme.fg)
                    }
                    VStack(alignment: .leading, spacing: 6) {
                        Eyebrow("Private on your device")
                        Text("Messages arrive while this app is open, and when iOS allows a background refresh.")
                            .font(Theme.sans(13)).foregroundStyle(Theme.muted)
                    }
                }.padding(.horizontal, 16).padding(.top, 8).padding(.bottom, 32)
            }
            .scrollIndicators(.hidden)
            .background(Theme.bg.ignoresSafeArea())
            .scrollDismissesKeyboard(.interactively)
            .toolbar { ToolbarItemGroup(placement: .keyboard) { Spacer(); Button("Done") { UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil) } } }
        }
    }

    private func qrImage(_ value: String) -> UIImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(value.utf8)
        guard let output = filter.outputImage,
              let cg = CIContext().createCGImage(output, from: output.extent) else { return nil }
        return UIImage(cgImage: cg)
    }
}
