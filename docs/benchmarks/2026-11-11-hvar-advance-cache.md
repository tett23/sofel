# 可変フォントの水平送り幅キャッシュの計測（2026-11-11）

[ADR 0049](../adr/0049-hvar-advance-cache.md) の128枠キャッシュを、macOS arm64 の release ビルドで測った。比較対象は同じ入力を処理するキャッシュなしのシェーピングと HarfBuzz。

## シェーピング

各テキストを行単位でシェーピングした合計時間を示す。キャッシュなし（A）とあり（B）のバイナリを A・B・B・A の順で交互に走らせた。各条件の4回の中央値を µs で示す。HarfBuzz は B と同時に測った。

| テキスト・位置 | フォント | A | B | 増減 | HarfBuzz |
| --- | --- | ---: | ---: | ---: | ---: |
| react-dom.txt default | RobotoFlex-Variable | 152,927 | 153,422 | +0.3% | 72,431 |
| react-dom.txt wght=500 | RobotoFlex-Variable | 255,271 | 208,503 | -18.3% | 71,167 |
| en-thelittleprince.txt default | RobotoFlex-Variable | 13,128 | 13,256 | +1.0% | 5,317 |
| en-thelittleprince.txt wght=500 | RobotoFlex-Variable | 21,917 | 17,022 | -22.3% | 6,162 |
| react-dom.txt default | SourceSerifVariable-Roman | 210,948 | 211,401 | +0.2% | 71,558 |
| react-dom.txt wght=500 | SourceSerifVariable-Roman | 276,996 | 247,931 | -10.5% | 79,794 |
| en-thelittleprince.txt default | SourceSerifVariable-Roman | 18,007 | 17,555 | -2.5% | 5,594 |
| en-thelittleprince.txt wght=500 | SourceSerifVariable-Roman | 23,071 | 20,213 | -12.4% | 6,261 |
| react-dom.txt default | AdobeVFPrototype | 130,797 | 129,809 | -0.8% | 62,682 |
| react-dom.txt wght=500 | AdobeVFPrototype | 200,938 | 169,721 | -15.5% | 73,298 |
| en-thelittleprince.txt default | AdobeVFPrototype | 10,506 | 10,406 | -0.9% | 5,146 |
| en-thelittleprince.txt wght=500 | AdobeVFPrototype | 15,910 | 13,128 | -17.5% | 5,925 |

## 送り幅と初期化

全グリフを順に1回ずつ引く場合と、末尾のグリフをグリフ数と同じ回数引く場合を測った。キャッシュありでは反復ごとに空のキャッシュから始める。各欄は3回の中央値で、単位は µs。

| 位置・順序 | フォント | キャッシュなし | 128枠 | HarfBuzz |
| --- | --- | ---: | ---: | ---: |
| sequential wght=500 | RobotoFlex-Variable | 87.6 | 87.9 | 68.0 |
| repeated wght=500 | RobotoFlex-Variable | 20.6 | 0.7 | 17.9 |
| sequential default | RobotoFlex-Variable | 2.0 | 2.5 | 7.6 |
| repeated default | RobotoFlex-Variable | 2.0 | 0.6 | 7.6 |
| sequential wght=500 | SourceSerifVariable-Roman | 54.3 | 55.0 | 48.6 |
| repeated wght=500 | SourceSerifVariable-Roman | 54.4 | 1.0 | 26.7 |
| sequential default | SourceSerifVariable-Roman | 3.0 | 3.7 | 11.5 |
| repeated default | SourceSerifVariable-Roman | 2.9 | 1.0 | 11.4 |
| sequential wght=500 | AdobeVFPrototype | 12.8 | 13.1 | 7.8 |
| repeated wght=500 | AdobeVFPrototype | 11.9 | 0.3 | 5.9 |
| sequential default | AdobeVFPrototype | 0.6 | 0.8 | 2.5 |
| repeated default | AdobeVFPrototype | 0.6 | 0.2 | 2.5 |

確保・初期化・解放を1000回繰り返した時間は 33.5〜38.7 µs だった（1回あたり同じ数値の ns）。非既定位置のシェーピングでキャッシュ用の Box を一つ確保する。128枠の配列は1024バイトで、Box の中には配列のほかに Instance が入る。既定位置のシェーピングでキャッシュ用の確保は0回。

## 再計測

入力を用意した状態で、同じ release ビルドの項目を次のコマンドで実行する。キャッシュなしのシェーピングとの比較には、変更前の実装（5cb8f8d）に同じシェーピング用計測項目を載せたバイナリを使った。

```sh
cargo run --release --manifest-path bench/Cargo.toml -- --filter 'ADR0049 shaping' --testdata target/testdata
cargo run --release --manifest-path bench/Cargo.toml -- --filter 'ADR0049 advance' --testdata target/testdata
```
