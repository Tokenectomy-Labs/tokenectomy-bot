# Tokenectomy Bot — Verifiable Benchmark Audit

Dokumentasi benchmark performa dan presisi deterministik untuk `tokenectomy-bot`.

> **Prinsip**: *Extreme Verifiable Engineering* — Tidak ada klaim performa tanpa pembuktian kode uji mandiri yang dapat diaudit langsung pada perangkat keras lokal.

---

## 1. Hasil Uji Audit 1.000 Kasus (Hardware-Grounded)

Dijalankan langsung menggunakan suite uji rilis:
```bash
cargo test --release --test stress_1k_benchmark -- --nocapture
```

### Metrik Eksekusi

| Metrik | Hasil Audit | Target Roadmap | Status |
|---|---|---|---|
| **Total Test Cases** | **1.000 berkas diff** | 1.000 | **LULUS** |
| **Precision Rate** | **100.00%** | $\ge 95.00\%$ | **LULUS** |
| **False Positives** | **0** | 0 | **LULUS** |
| **False Negatives** | **0** | 0 | **LULUS** |
| **Zero Panics / Crashes** | **100% stabil** | 0 panic | **LULUS** |
| **Total Execution Time** | **550.02 ms** | < 3.000 ms | **Unggul (0.55 detik)** |
| **Rata-rata Latensi per Berkas** | **0.550 ms** | < 5.0 ms | **Sub-millisecond** |
| **Throughput Engine** | **1.818,1 PR files/detik** | > 200 files/detik | **9x lebih cepat** |

---

## 2. Rincian Distribusi 1.000 Kasus Uji

```
Distribusi 1.000 Kasus Uji
├── 100 Kasus Adversarial & DoS Probes (Header rusak, Emoji/Unicode, CRLF, Binary, Syntax Error)
├── 250 Kasus Clean Production Code (Probe False Positive: Logger sah, ORM paginasi, Real Assertion)
├── 350 Kasus True Positives (TB001, TB002, TB003, TB004, TB005, TB006, TB009, TB101, TB102, TB104, TB201, TB202, TB203)
├── 150 Kasus Inline Suppression (Komentar `// tokenectomy-ignore: TBxxx`)
└── 150 Kasus Baseline Filtering (Hutang teknis terdaftar di `.tokenectomy-baseline.json`)
```

---

## 3. Temuan Kritis & Hardening Perangkat Lunak

Selama pengujian stress adversarial, ditemukan dan diperbaiki satu kerentanan DoS alokasi memori:
* **Vulnerability**: Diff dengan nomor baris ekstrem (misal `@@ -99999999,1 +99999999,2 @@`) memicu alokasi vektor string hingga 100 juta elemen (~2.4 GB RAM).
* **Hardening**: Penerapan pembatas alokasi aman `pub const MAX_SYNTHETIC_LINES: usize = 100_000;` pada crate `tb-diff` untuk menghentikan serangan DoS berbasis diff palsu secara instan.
* **Protokol Penanganan**: Analisis dan pembersihan frame log dilakukan secara otomatis menggunakan `tokenectomy --scrub`.

---

## 4. Cara Mereproduksi Audit

Untuk memverifikasi benchmark ini sendiri secara independen:

```bash
# 1. Clone repository
git clone https://github.com/Tokenectomy-Labs/tokenectomy-bot
cd tokenectomy-bot

# 2. Jalankan benchmark presisi 12 kasus inti
cargo test --release --test precision_benchmark -- --nocapture

# 3. Jalankan benchmark ketahanan 1.000 kasus penuh
cargo test --release --test stress_1k_benchmark -- --nocapture
```
