# ADR 0057: 幻の点による輪郭の移動を除外なしで照合し、結果を記録する

ステータス: 採択（未実装）

## 文脈

[ADR 0018](0018-phantom-point-shift.md) は、TrueType の幻の点 `pp1.x` に基づいてグリフの輪郭を移動する処理を定めた。
そのテスト設計では、ベンチマークの `--once` で対象グリフを除外せずに照合する。
[ロードマップ](../roadmap.md)には、この照合の記録を残すまで ADR のステータスを戻すかという判断が残っている。

## 調査した事項

[2段階目の計測記録](../benchmarks/2026-11-10-stage2.md)は、ラスタライズの照合で Roboto-Regular の52グリフと Comfortaa-Regular-new の6グリフを除外したと記している。

現在のラスタライズの照合は、各フォントのグリフ番号を先頭から末尾まで sofel と FreeType の両方に渡す。
12、24、48、96 ppem（1 em あたりのピクセル数）のそれぞれで、全グリフにわたる画素値の合計を比較する。
CI のベンチマークジョブはネイティブと wasm の両方で `--once` を実行する。

## 実装すること

[ADR 0018](0018-phantom-point-shift.md) のステータスは「採択」とする。
除外なしの照合とその記録を本 ADR の作業として扱う。

同じコミットと固定されたテストデータを使い、CI のベンチマークジョブと同じ条件で次の照合を行う。
[CI のベンチマークジョブ](../../.github/workflows/ci.yml)に従い、wasmtime、WASI SDK、Rust の wasm32-wasip1 ターゲットを用意する。
`scripts/fetch-testdata.sh` と `scripts/fetch-fonts.sh` で固定入力を用意し、`scripts/build-bench-c.sh` で両環境の参照ライブラリをビルドする。
作業ディレクトリはリポジトリのルートとし、入力を限定するフィルターは指定しない。

```sh
cargo run --release --manifest-path bench/Cargo.toml -- --once
cargo build --release --manifest-path bench/Cargo.toml --target wasm32-wasip1
wasmtime run -W exceptions=y --dir target/testdata bench/target/wasm32-wasip1/release/sofel-bench.wasm --once
```

実行日を名前に含む新しい Markdown ファイルを `docs/benchmarks/` に作り、対象コミット、入力データの版、実行環境、コマンド、各環境の終了コードを記録する。
Roboto-Regular と Comfortaa-Regular-new について、対象グリフ数、以前の除外数、今回の除外数、各 ppem での照合結果を記載する。
不一致や実行できなかった項目があれば、対象と理由を記録する。
両環境で照合が通った記録を追加した時点で、ロードマップにその記録へのリンクを載せる。

## 実装しないこと

過去の計測記録の改訂、輪郭の移動規則の変更、実行時間の優劣の判定は対象外とする。

## 不変条件

なし。この ADR が定めるのは既存の照合を実行して記録する手順である。

## テスト設計

対象コミットで、両フォントのラスタライズ照合が両実装に全グリフ番号を渡し、`pp1.x` による除外が0件であることを確かめる。
成功した個別項目は一覧に出ないため、標準エラーに出る両フォントのグリフ数、対象コミットの4サイズのループ、フィルターなしの実行引数を照合する。
各環境で不一致の報告がなく、`--once` が終了コード0を返したことを確かめる。
一方の結果が欠けている間、または不一致が残っている間は、照合を完了としない。

## トレードオフ

ネイティブと wasm の両方を実行する分だけ時間がかかるが、以前の記録と同じ二つの環境で除外を解消した結果を残せる。
この照合で確認できるのはラスタライズ項目の集計値の一致までであり、グリフごとの各画素の一致は別の検査を要する。

## 参考文献

- [ベンチマークの照合処理](../../bench/src/main.rs)
- [CI のベンチマークジョブ](../../.github/workflows/ci.yml)
- [計測の方法を定めた ADR 0013](0013-benchmark-harness.md)
