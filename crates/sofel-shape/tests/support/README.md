# 自作のテスト入力

このディレクトリの入力は sofel のテスト用に一から作り、sofel と同じライセンスで配布する。

## type1_triangle.pfa

`crates/sofel-type1/tests/support/type1_triangle.rs` の `sample()` が返す最初の値である。
この関数は `.notdef` と三角形の `A` を組み、Type1 の暗号化と ASCII の符号化を行う。
次のコマンドをリポジトリのルートで実行すると、同じバイト列を作り直せる。

```sh
work=$(mktemp -d)
cat > "$work/generate.rs" <<EOF_RUST
#[path = "$(pwd)/crates/sofel-type1/tests/support/type1_triangle.rs"]
mod fixture;
fn main() {
    std::fs::write("crates/sofel-shape/tests/support/type1_triangle.pfa", fixture::sample().0).unwrap();
}
EOF_RUST
rustc --edition 2024 "$work/generate.rs" -o "$work/generate"
"$work/generate"
rm -r "$work"
```
