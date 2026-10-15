import SwiftUI
import Charts

struct ActivityDay: Decodable, Identifiable, Sendable {
    let date: String; let reading: UInt64; let listening: UInt64
    var id: String { date }
}
struct ActivityBook: Decodable, Identifiable, Sendable {
    let hash: String; let title: String; let reading: UInt64; let listening: UInt64; let undated: UInt64
    var id: String { hash }
}
struct ActivityReport: Decodable, Sendable {
    let today: UInt64; let week: UInt64; let total: UInt64; let reading: UInt64; let listening: UInt64
    let undated: UInt64; let active_days: Int; let streak: Int; let days: [ActivityDay]; let books: [ActivityBook]; let devices: Int
}
extension CoreStore {
    func activity(_ fields: [String: Any]) throws { let _: [String: Bool] = try client.call("activity_checkpoint", fields) }
    func activityReport(id: String?) throws -> ActivityReport {
        let formatter = DateFormatter(); formatter.locale = Locale(identifier: "en_US_POSIX"); formatter.dateFormat = "yyyy-MM-dd"
        var fields: [String: Any] = ["today": formatter.string(from: Date())]
        if let id { fields["id"] = id }
        return try client.call("activity_report", fields)
    }
}
@MainActor final class ReadingActivity: ObservableObject {
    let store: CoreStore; let id: String
    private let session = UUID().uuidString
    private var last = ProcessInfo.processInfo.systemUptime
    private var touched = ProcessInfo.processInfo.systemUptime
    private var mode: Int?
    private var totals = [0.0, 0.0]
    private var saved: [UInt64] = [0, 0]
    @Published private(set) var error: String?
    init(library: PreviewLibrary, id: String) { store = library.store; self.id = id }
    func touch() { touched = ProcessInfo.processInfo.systemUptime }
    func sample(reading: Bool, listening: Bool) {
        let now = ProcessInfo.processInfo.systemUptime, delta = now - last
        if let mode, delta >= 0 && delta <= 2 { totals[mode] += delta }
        last = now
        let next: Int? = listening ? 1 : reading && now - touched < 300 ? 0 : nil
        if next != mode || (0..<2).contains(where: { UInt64(totals[$0]) - saved[$0] >= 15 }) { flush() }
        mode = next
    }
    func stop() { sample(reading: false, listening: false); flush() }
    func flush() {
        let date = Date(), timestamp = UInt64(date.timeIntervalSince1970), offset = TimeZone.current.secondsFromGMT(for: date)
        for index in 0..<2 {
            let elapsed = UInt64(totals[index]); guard elapsed > saved[index] else { continue }
            let fields: [String: Any] = ["id": id, "session": session + String(index), "elapsed": elapsed,
                "timestamp": timestamp, "offset": offset, "listening": index == 1]
            saved[index] = elapsed
            let background = UIApplication.shared.beginBackgroundTask(withName: "Save reading activity")
            Task {
                do { try await store.activity(fields) } catch { self.error = L("common.error_activity_load") }
                if background != .invalid { UIApplication.shared.endBackgroundTask(background) }
            }
        }
    }
}
private func activityDuration(_ seconds: UInt64) -> String {
    if seconds >= 3600 { return L("stats.hours_minutes", seconds / 3600, seconds % 3600 / 60) }
    if seconds >= 60 { return L("stats.minutes", seconds / 60) }
    return L("stats.seconds", seconds)
}
struct ReadingStatsView: View {
    @ObservedObject private var readerI18n = ReaderI18n.shared
    @ObservedObject var library: PreviewLibrary
    var currentID: String? = nil
    var syncSettings: (() -> Void)? = nil
    @State private var current = true
    @State private var range = 7
    @State private var report: ActivityReport?
    @State private var selected: String?
    @State private var failure: String?
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if currentID != nil { Picker(L("stats.scope"), selection: $current) { Text(L("ui.all_books")).tag(false); Text(L("stats.current_book")).tag(true) }.pickerStyle(.segmented) }
                if let failure { Text(LM(failure)).foregroundStyle(.red) }
                if let report {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(L("stats.today")).foregroundStyle(.secondary)
                        Text(activityDuration(report.today)).font(.system(size: 34, weight: .bold, design: .rounded)).monospacedDigit()
                        Text(L("stats.last_7_days_summary", activityDuration(report.week), activityDuration(report.total))).font(.subheadline).foregroundStyle(.secondary)
                    }
                    if report.total == 0 {
                        VStack(alignment: .leading, spacing: 8) { Text(L("stats.empty")).font(.headline); Text(L("stats.empty_hint")).foregroundStyle(.secondary) }
                    } else {
                        VStack(alignment: .leading, spacing: 12) {
                            HStack { Text(L("stats.trend")).font(.headline); Spacer(); Picker(L("stats.date_range"), selection: $range) { Text(L("stats.week")).tag(7); Text(L("stats.thirty_days")).tag(30) }.pickerStyle(.segmented).frame(maxWidth: 190) }
                            let days = Array(report.days.suffix(range))
                            Chart(days) { day in
                                BarMark(x: .value(L("stats.date"), day.date), y: .value(L("stats.unit_minutes"), Double(day.reading) / 60)).foregroundStyle(by: .value(L("stats.type"), L("stats.reading")))
                                BarMark(x: .value(L("stats.date"), day.date), y: .value(L("stats.unit_minutes"), Double(day.listening) / 60)).foregroundStyle(by: .value(L("stats.type"), L("stats.listening")))
                            }
                            .chartForegroundStyleScale([L("stats.reading"): Color.accentColor, L("stats.listening"): Color.orange])
                            .chartXSelection(value: $selected)
                            .chartYAxisLabel(L("stats.unit_minutes"))
                            .chartXAxis {
                                AxisMarks(values: days.enumerated().filter { $0.offset % (range == 7 ? 1 : 7) == 0 }.map { $0.element.date }) { value in
                                    AxisValueLabel { if let date = value.as(String.self) { Text(String(date.suffix(5))).font(.caption2) } }
                                }
                            }
                            .frame(height: 180)
                            if let day = days.first(where: { $0.date == selected }) { Text(L("stats.summary_full", day.date, activityDuration(day.reading), activityDuration(day.listening))).font(.caption) }
                            Text(L("stats.summary_short", activityDuration(report.reading), activityDuration(report.listening))).font(.subheadline).monospacedDigit()
                            Text(L("stats.active_streak", report.active_days, report.streak)).font(.caption).foregroundStyle(.secondary)
                        }
                        VStack(alignment: .leading, spacing: 14) {
                            Text(L("stats.books")).font(.headline)
                            ForEach(report.books) { book in
                                HStack(alignment: .firstTextBaseline) { Text(book.title).frame(maxWidth: .infinity, alignment: .leading); Text(activityDuration(book.reading + book.listening + book.undated)).monospacedDigit() }
                                Divider()
                            }
                        }
                    }
                    DisclosureGroup(L("stats.rules")) {
                        VStack(alignment: .leading, spacing: 10) {
                            Text(L("stats.timer_hint"))
                            Text(L("stats.sync_hint"))
                            Text(L("stats.date_hint"))
                        }.font(.caption).foregroundStyle(.secondary).padding(.top, 8)
                    }
                    if let syncSettings { Button(L("stats.sync_settings"), action: syncSettings) }
                } else if failure == nil { ProgressView().frame(maxWidth: .infinity).padding(32) }
            }.padding(20)
        }.background(Color(uiColor: .systemGroupedBackground)).navigationTitle(L("stats.title")).navigationBarTitleDisplayMode(.inline)
        .task(id: current) {
            report = nil; failure = nil; selected = nil
            while !Task.isCancelled {
                do { let value = try await library.store.activityReport(id: current ? currentID : nil); guard !Task.isCancelled else {return}; report=value;failure=nil }
                catch { if !Task.isCancelled {failure = L("common.error_activity_load")} }
                do {try await Task.sleep(for: .seconds(15))} catch {return}
            }
        }
    }
}
