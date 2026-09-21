import CoreImage.CIFilterBuiltins
import SwiftUI

struct PairingView: View {
    @EnvironmentObject private var store: ChatStore
    @State private var phone = ""
    @State private var showQR = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 28) {
                    VStack(alignment: .leading, spacing: 14) {
                        Image("Brand").resizable().frame(width: 78, height: 78).clipShape(RoundedRectangle(cornerRadius: 20))
                        Text("Your chats.\nOn your iPhone.").font(.largeTitle.bold())
                        Text("Link your existing WhatsApp account to this independent companion.")
                            .font(.body).foregroundStyle(.secondary)
                    }
                    if let code = store.pairingCode {
                        VStack(alignment: .leading, spacing: 18) {
                            Text("Enter this code in WhatsApp").font(.headline)
                            Text(code).font(.system(size: 32, weight: .semibold, design: .monospaced))
                                .tracking(3).textSelection(.enabled).accessibilityIdentifier("pairing-code")
                            Button {
                                UIPasteboard.general.setItems([[UIPasteboard.typeAutomatic: code]], options: [
                                    .localOnly: true, .expirationDate: Date().addingTimeInterval(120)
                                ])
                            } label: {
                                Label("Copy code", systemImage: "doc.on.doc")
                            }.buttonStyle(.borderedProminent)
                            Text("Switch to official WhatsApp, enter the code and approve. Return here straight after submitting it so this app can finish connecting.")
                                .font(.subheadline).foregroundStyle(.secondary)
                            Text("iOS gives linking limited time while you switch apps. This code stops working if that time runs out.")
                                .font(.footnote).foregroundStyle(.secondary)
                        }
                        .padding(20).background(.regularMaterial, in: RoundedRectangle(cornerRadius: 20))
                    } else {
                        VStack(alignment: .leading, spacing: 14) {
                            if store.pairingInterrupted {
                                Label("Linking was interrupted", systemImage: "clock.badge.exclamationmark").font(.headline)
                                Text("This app ran out of background time, so the previous code is no longer active. Request a fresh code and return here straight after submitting it in WhatsApp.")
                                    .font(.subheadline).foregroundStyle(.secondary)
                                    .accessibilityIdentifier("pairing-interrupted")
                            }
                            Text("First, open official WhatsApp → Settings → Linked Devices → Link a Device → Link with phone number instead. Then return here to get your code.")
                                .font(.subheadline).foregroundStyle(.secondary)
                            Text("Phone number").font(.headline)
                            TextField("Country code + phone number", text: $phone)
                                .keyboardType(.phonePad).textContentType(.telephoneNumber)
                                .padding(16).background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 14))
                                .accessibilityIdentifier("pairing-phone")
                            Button { store.pair(phone: phone) } label: {
                                HStack {
                                    if store.pairingBusy { ProgressView().tint(.black) }
                                    Text(store.pairingBusy ? "Getting your code…" : "Link with phone number").fontWeight(.semibold)
                                }.frame(maxWidth: .infinity).padding(.vertical, 8)
                            }
                            .buttonStyle(.borderedProminent).buttonBorderShape(.roundedRectangle(radius: 14))
                            .foregroundStyle(.black).disabled(store.pairingBusy || store.status != "unlinked")
                            .accessibilityIdentifier("pairing-submit")
                            Text("Include the country code, for example +44. This adds a linked device; your existing account stays on your phone.")
                                .font(.footnote).foregroundStyle(.secondary)
                        }
                    }
                    if store.status == "starting" || store.status == "connecting" {
                        HStack(spacing: 10) { ProgressView(); Text("Connecting securely…").foregroundStyle(.secondary) }
                    } else if store.status == "failed" || store.status == "disconnected" || store.status == "logged_out" {
                        Button("Try connecting again") { store.reconnect() }
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
                                    .font(.footnote).foregroundStyle(.secondary)
                            }.frame(maxWidth: .infinity).padding(.top, 14)
                        }
                    }
                    Label("Private on your device", systemImage: "lock.shield")
                        .font(.subheadline.weight(.medium)).foregroundStyle(.secondary)
                    Text("Messages arrive while this app is open. Background notifications are not supported yet.")
                        .font(.footnote).foregroundStyle(.secondary)
                }.padding(28)
            }
            .background(Color(.systemGroupedBackground))
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
