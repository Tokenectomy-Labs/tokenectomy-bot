# Roadmap: tokenectomy-bot (v2)

**Gerbang verifikasi Pull Request di era AI agent.** Peninjau PR deterministik berbasis AST (Tree-sitter) yang menangkap bug struktural, celah keamanan, dan, ini pembedanya, **kecurangan agent yang meloloskan test tanpa memperbaiki kode**, sebelum review manusia.

Posisi di ekosistem: Tokenectomy memangkas noise log sebelum masuk LLM, Tokenectomy Git membuka PR dari agent, **tokenectomy-bot memverifikasi PR itu di sisi penerima**. Lingkarannya tertutup.

---

## Prinsip Desain

1. **Presisi di atas recall.** Satu false positive merusak kepercayaan lebih parah daripada satu bug yang terlewat. Rule baru tidak boleh memblokir merge sebelum lolos precision gate (lihat Tahap 2).
2. **Inti deteksi 100% deterministik, zero-LLM.** LLM hanya boleh dipakai untuk menjelaskan dan menyarankan perbaikan, tidak pernah untuk memutuskan lulus/gagal.
3. **Satu binary Rust.** Tree-sitter native, cold start cepat, tanpa runtime Node di CI, dan bisa memakai ulang logika Sentinel yang sudah ada.
4. **Tanpa rahasia tambahan.** Berjalan hanya dengan `GITHUB_TOKEN` bawaan runner; degradasi anggun ketika izin tidak cukup (PR dari fork).
5. **Bisa diaudit.** Setiap klaim (presisi, kecepatan) punya benchmark yang bisa dijalankan siapa pun, sama seperti stress benchmark Tokenectomy.

## Jalur Kerja Paralel

Roadmap dibagi menjadi tahap berurutan, tetapi pekerjaan dijalankan dalam empat jalur yang bisa berjalan bersamaan:

| Jalur | Cakupan |
|---|---|
| **Engine** | diff, parsing, mapping, model finding, performa |
| **Rules** | aturan deteksi, fixture, corpus presisi |
| **Integrations** | GitHub Action, reporter, MCP, distribusi |
| **Docs & Growth** | dokumentasi rule, benchmark publik, adopsi |

---

## Tahap 1: Walking Skeleton (Core Engine + End-to-End Sejak Awal)

**Tujuan:** pipeline penuh berjalan di repo Tokenectomy sendiri (dogfooding), meski baru dengan satu rule. Semua tahap berikutnya hanya menambah isi ke pipa yang sudah hidup.

- [ ] **Workspace Rust & CI**
  - Cargo workspace dengan crate terpisah: `tb-diff`, `tb-parse`, `tb-rules`, `tb-report`, `tb-cli`.
  - CI build + test di Linux/macOS/Windows; `clippy` dan `rustfmt` wajib lulus.
- [ ] **Git Diff Extractor (dua sisi)**
  - Ambil hunk dari `git diff -U0 --no-color --find-renames <merge-base>...<head>` (three-dot, bukan two-dot).
  - Simpan **baris ditambah dan baris dihapus** beserta rentang lama dan baru. Aturan anti-tampering butuh sisi lama.
  - Tangani: rename, file dihapus, file biner, perubahan mode, CRLF, encoding non-UTF-8, path dengan spasi/unicode.
- [ ] **Klasifikasi & Filter Berkas**
  - Kelas: `source`, `test`, `config`, `ci`, `generated`, `ignored`.
  - Abaikan: lockfile, `dist/`, `build/`, `vendor/`, `node_modules/`, `*.min.js`, hasil generate.
  - **Berkas test tidak dibuang.** Ia diklasifikasikan sebagai `test` dan hanya dipindai oleh rule integritas (TB0xx), bukan rule kualitas kode biasa.
- [ ] **Tree-sitter Parsing (sisi lama & baru)**
  - Grammar awal: TypeScript, TSX, JavaScript, JSX.
  - Parse blob lama (`git show base:path`) dan baru; toleran terhadap error sintaks (partial tree, jangan panik).
- [ ] **Range Mapping (semantik overlap)**
  - Node dilaporkan jika rentangnya **beririsan** dengan rentang yang berubah, bukan hanya jika seluruhnya berada di dalam baris tambahan. Ini menangkap bug akibat penghapusan (misal hilangnya `await` atau `limit`) yang membuat kode lama yang tak disentuh ikut salah.
  - Rule dapat menyatakan `needs_old_side: true` untuk membandingkan pohon lama vs baru.
- [ ] **Model Finding & Fingerprint**
  - Field: `rule_id`, `severity`, `confidence`, `file`, `start/end line`, `message`, `fix_hint`, `fingerprint`.
  - `fingerprint` stabil terhadap pergeseran nomor baris (hash dari rule + struktur node + konteks), dipakai untuk dedupe, baseline, dan pelacakan komentar.
- [ ] **Reporter Dasar**
  - Keluaran JSON, SARIF 2.1.0, dan GitHub workflow annotations (`::warning file=,line=::`).
  - Kebijakan exit code terpisah: `0` bersih/peringatan, `1` diblokir oleh temuan, `2` galat internal alat.
- [ ] **Action Wrapper Minimal**
  - Composite action yang mengunduh binary rilis (terverifikasi checksum) dan menjalankannya pada PR.
  - Dipasang di repo Tokenectomy sendiri sejak hari pertama.
- [ ] **Harness Uji**
  - Fixture berbasis (diff masukan → temuan yang diharapkan) dengan snapshot test.
  - Fuzzing (`cargo-fuzz`) untuk parser diff dan pemetaan rentang: tidak boleh ada panic pada masukan apa pun.

**Kriteria selesai:** PR nyata di repo Tokenectomy memunculkan annotation dari satu rule contoh; tidak ada panic pada fuzz 1 juta iterasi; p95 < 3 detik untuk PR 500 berkas.

---

## Tahap 2: Aturan Deteksi (dengan Precision Gate)

Kode rule: **TB0xx** integritas verifikasi, **TB1xx** reliabilitas, **TB2xx** keamanan.

### Rule 0: Integritas Verifikasi (pembeda utama)

Mendeteksi trik agent yang membuat CI hijau tanpa memperbaiki perilaku. Ini melanjutkan pekerjaan Sentinel ke sisi PR.

- [ ] **TB001 `assertion-removed`**: jumlah assertion turun bersih di dalam test block yang masih ada (bukan test yang dihapus utuh).
- [ ] **TB002 `test-disabled`**: penambahan `.skip`, `xit`, `xdescribe`, `it.todo`, `test.fixme`, dan padanannya (`#[ignore]`, `@Ignore`) pada tahap bahasa berikutnya.
- [ ] **TB003 `tautological-assertion`**: `expect(true).toBe(true)`, `assert(1 === 1)`, membandingkan nilai dengan dirinya sendiri.
- [ ] **TB004 `config-weakened`**: perubahan yang melemahkan verifikasi: `testPathIgnorePatterns` bertambah, ambang coverage turun, `passWithNoTests`, `strict: false` di tsconfig, aturan lint dimatikan, `continue-on-error: true` atau langkah test dihapus di workflow CI.
- [ ] **TB005 `early-exit-injected`**: `return`/`process.exit(0)`/`throw` baru yang membuat sisa fungsi atau test tak terjangkau.
- [ ] **TB006 `test-deleted-with-source-change`** (severity `info`): test dihapus dalam PR yang juga mengubah kode sumber terkait.
- [ ] **TB007 `lazy-deletion`**: fungsi atau cabang `if` dihapus, atau body diganti `return null`, `todo!()`, `throw new Error("not implemented")`, sementara pemanggilnya tetap ada.
- [ ] **TB008 `domain-narrowing`**: guard baru yang hanya meloloskan literal yang sama dengan nilai di test (special-casing agar test hijau, bukan perbaikan perilaku).
- [ ] **TB009 `fixture-snooping`**: kode produksi membaca `fixtures/` atau `__tests__`, atau bercabang pada `NODE_ENV === 'test'` di dalam logika bisnis.
- [ ] **Deteksi PR buatan agent** (menaikkan severity, bukan syarat aktif): prefiks branch, trailer `Co-authored-by`, label PR, akun bot. Mode `agent-pr` menjadikan TB001–TB005 berstatus `error`.

### Rule 1: Reliabilitas

- [ ] **TB101 `silent-catch`**
  - Temukan: `catch` kosong atau hanya berisi komentar, `.catch(() => {})`, `.catch(noop)`; di Rust hanya `Err(_) => {}` (bukan `_ => {}` yang sering sah).
  - **Tidak** dilaporkan bila ada: log (nama logger bisa dikonfigurasi), `throw`/re-throw, `return`, atau penugasan yang jelas menangani galat.
- [ ] **TB102 `unbounded-query`**
  - Deteksi klien ORM lewat **import** (Prisma, Drizzle, TypeORM, Sequelize, Mongoose, Knex), bukan sekadar nama method. `.all()` pada `Map` atau array tidak boleh terkena.
  - Laporkan `findMany()` tanpa `take`, `select().from()` tanpa `.limit()`, `find()` tanpa `take`/`limit`, dan sejenisnya.
  - Confidence bertingkat: `high` jika klien terbukti dari import, `low` jika hanya dari pola nama (tidak pernah memblokir).
- [ ] **TB103 `floating-promise`**
  - Tingkat (a): pemanggilan fungsi yang **terbukti async** dari deklarasi di berkas yang sama atau import satu lompatan di dalam repo.
  - Tingkat (b): API async yang dikenal (`fetch`, `fs.promises.*`, `Promise.*`).
  - Dikecualikan: `await`, `return`, `void`, `.catch()`, `.then(_, onRejected)`, argumen `Promise.all/allSettled/race`.
  - Mode berbasis tipe (memakai `tsc`) menjadi opsi terpisah, bukan syarat.
- [ ] **TB104 `async-foreach`**: `arr.forEach(async ...)` yang hasilnya tidak ditunggu. Bug klasik dengan false positive sangat rendah.
- [ ] **TB105 `await-in-loop`** (performa): `await` pada panggilan DB/HTTP yang terbukti (klien dari import) di dalam loop, yaitu pola N+1. Severity awal `warn`, confidence `high` hanya jika klien terbukti.

### Rule 2: Keamanan

- [ ] **TB201 `dynamic-eval`**: `eval()`, `new Function()`, `setTimeout("string")`, `vm.runIn*Context`.
- [ ] **TB202 `shell-injection`**: `exec`/`execSync` dengan string hasil interpolasi atau konkatenasi dari nilai non-literal.
- [ ] **TB203 `raw-sql-interpolation`**: `$queryRawUnsafe`, `.query()` dengan template literal berisi variabel.

### Tata Kelola Kualitas Rule

- [ ] **Siklus hidup rule:** `experimental` (tidak ditampilkan) → `warn` → `error`. Kenaikan status wajib melewati precision gate.
- [ ] **Precision gate:** jalankan pada PR yang sudah di-merge dari ≥50 repo TypeScript populer, label manual sampel temuan, syarat **presisi ≥ 95%** untuk `error`.
- [ ] **Benchmark yang bisa direproduksi:** `cargo test --release --test precision_benchmark`, hasil dipublikasikan di repo.
- [ ] **Dokumen per rule:** kenapa berbahaya, contoh salah dan benar, cara memperbaiki, cara menekan temuan.
- [ ] **Mekanisme penekanan:** komentar `// tokenectomy-ignore: TB101 -- alasan`, ignore per path, dan berkas baseline (`.tokenectomy-baseline.json`, berbasis fingerprint) supaya repo lama bisa mulai bersih dari hari pertama.

**Kriteria selesai:** semua rule TB0xx dan TB1xx punya fixture positif dan negatif; presisi terukur dan terpublikasi; tidak ada rule berstatus `error` tanpa lulus gate.

---

## Tahap 3: Integrasi CI/CD & Pelaporan

- [ ] **Distribusi Binary di Action**
  - Rilis binary untuk linux x64/arm64, macOS, Windows; checksum SHA-256 diverifikasi saat diunduh; cache antar-run.
- [ ] **Izin & Strategi PR dari Fork**
  - Dokumentasikan `permissions: contents: read, pull-requests: write`.
  - `GITHUB_TOKEN` bersifat read-only pada PR dari fork, sehingga default jatuh ke **annotations + `$GITHUB_STEP_SUMMARY`**.
  - Pola opsional dua workflow: analisis di `pull_request` (tidak tepercaya) menghasilkan artifact, lalu workflow `workflow_run` yang memposting komentar. **Jangan pernah** checkout kode head dengan rahasia di `pull_request_target`.
- [ ] **Inline Review Commenter**
  - Komentar hanya pada baris yang ada di diff (sisi kanan), karena selain itu API mengembalikan 422; temuan di luar diff masuk ke ringkasan.
  - Kirim sebagai **satu review batch**, bukan komentar satu per satu, agar tidak membanjiri notifikasi.
  - Dedupe lewat penanda tersembunyi berisi fingerprint; komentar yang sudah usang setelah push baru ditandai selesai atau diminimalkan.
  - Blok `suggestion` GitHub untuk perbaikan yang aman dan mekanis.
- [ ] **PR Summary Sticky Comment**
  - Satu komentar yang **di-update di tempat** setiap push (penanda tersembunyi), tidak menumpuk komentar baru.
  - Ketika temuan sudah nol, komentar berubah menjadi status bersih, bukan dibiarkan usang.
  - Isi: hitungan per rule dan severity, tabel temuan teratas, detail dilipat.
- [ ] **Check Run & Annotations**
  - Batch annotation 50 per panggilan API; ringkasan check run menampilkan status gerbang.
- [ ] **Kebijakan Gerbang**
  - `fail-on: error | warn | none`, override severity per rule, dan pemisahan galat alat (exit `2`).
  - `fail-open` secara default untuk galat internal (bot tidak boleh memblokir tim karena bug-nya sendiri); `fail-closed` opsional.
- [ ] **Ketahanan pada PR Besar**
  - Batas jumlah berkas dan ukuran per berkas, batas waktu, laporan terpotong yang jujur ("dipindai 480 dari 1.200 berkas").

**Kriteria selesai:** PR dari fork dan non-fork sama-sama menghasilkan umpan balik yang berguna; push berulang tidak menghasilkan komentar duplikat; galat internal tidak pernah memblokir merge secara default.

---

## Tahap 4: Konfigurasi, Distribusi & Bahasa Tambahan

- [x] **`tokenectomy.json`**
  - JSON Schema terpublikasi untuk autocomplete editor; validasi dengan pesan galat yang jelas (`schema.json`).
  - Opsi: aktif/nonaktif per rule, severity, `ignore` glob, nama logger untuk TB101, daftar ORM untuk TB102, override per path.
  - `extends` preset: `recommended`, `strict`, `agent-pr`.
  - Perintah `tokenectomy-bot init` untuk membuat konfigurasi awal.
- [x] **GitHub Marketplace Action**
  - `action.yml` dengan input/output terstandar, tag versi bergerak (`v1`) plus semver, otomatisasi rilis.
  - Provenance build (`attest-build-provenance`) dan SBOM workflow (`.github/workflows/release.yml`).
- [x] **Mode Lokal & Integrasi MCP**
  - `tokenectomy-bot review --base main` untuk dijalankan lokal.
  - Subcommand `tokenectomy-bot mcp` dan tool MCP `pre_pr_review` & `audit_diff` di ekosistem Tokenectomy: agent memeriksa diff-nya sendiri **sebelum** membuka PR, dan melihat temuan TB0xx pada dirinya sendiri.
- [x] **Distribusi Tambahan**
  - `cargo install`, multi-stage `Dockerfile`, dan workflow rilis multi-platform.
- [x] **Dukungan Bahasa**
  - Urutan: Rust → Go → Python dengan tree-sitter grammars.
  - Rule expansion untuk Rust (`attribute_item`, `macro_invocation`), Go (`t.Skip`, `if err != nil`, `assert.Equal`), dan Python (`decorator`, `except_clause`, `assert_statement`, `eval`).
  - Standar precision gate 100% dengan test case komprehensif.

**Kriteria selesai:** repo pihak ketiga bisa memasang bot hanya dengan satu berkas workflow dan nol konfigurasi; konfigurasi salah menghasilkan pesan galat yang bisa ditindaklanjuti.

---

## Tahap 5: Platform & Monetisasi

Menjadikan bot ini produk, bukan sekadar action.

- [ ] **GitHub App (berbasis webhook)**
  - Terpasang di tingkat organisasi tanpa workflow per repo; komentar dan check run dikirim langsung.
- [ ] **Rule Kustom**
  - Organisasi menulis rule sendiri sebagai query Tree-sitter (`.scm`) di `.tokenectomy/rules/`, dengan fixture wajib dan validator.
- [ ] **Kebijakan Tingkat Organisasi**
  - Preset yang dipaksakan lintas repo; pengecualian butuh persetujuan.
- [ ] **Ledger Audit**
  - Catatan tersegel SHA-256 atas temuan, penekanan, dan persetujuan per PR (gaya ledger Sovereign) untuk kebutuhan kepatuhan.
- [ ] **Dasbor & Metrik**
  - Percobaan tampering yang diblokir, tingkat lolos PR agent, rule terbanyak memicu, waktu review; notifikasi Slack/Teams.
- [ ] **Lapisan LLM Opsional (Sentinel Pro)**
  - Penjelasan temuan dan usulan perbaikan dengan konteks yang dikompresi Tokenectomy; auto-fix hanya berupa PR terpisah yang butuh persetujuan manusia.
  - Keputusan lulus/gagal tetap sepenuhnya deterministik.

---

## Metrik Keberhasilan

| Metrik | Target |
|---|---|
| Presisi rule berstatus `error` | ≥ 95% pada corpus publik |
| Waktu p95 (PR 500 berkas) | < 3 detik pada runner standar |
| Galat internal yang memblokir merge | 0 (default fail-open) |
| Temuan duplikat antar push | 0 |
| Adopsi | repo pihak ketiga aktif, instalasi Marketplace, PR agent terverifikasi |

## Risiko & Mitigasi

| Risiko | Mitigasi |
|---|---|
| False positive merusak reputasi | Precision gate, confidence bertingkat, `experimental` dulu |
| Terlihat seperti "ESLint lain" | Rule 0 (anti-tampering agent) sebagai pembeda dan kisah utama |
| Keterbatasan AST tanpa info tipe | Batasi lingkup ke pola terbukti; mode berbasis tipe opsional |
| Keamanan workflow (fork, token) | Pola dua workflow, tanpa `pull_request_target` + checkout head |
| Agent belajar mengakali rule | Ledger signature ala Adaptive Reflex Engine, rule baru dari kasus nyata |
