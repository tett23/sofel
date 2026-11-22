# 自作フォントからの参照出力

このディレクトリのファイルは、sofel のテストが一から組んだ入力を HarfBuzz 15.1.0（コミット `3df40d9b98d6b32f0e4288d4ecfbb158ddb3a372`）でサブセット化した出力である。
実在のフォントを元にしたものは含まない。

再生成には、リポジトリのルートで次の準備を行う。
`HB_DUMP` は、固定版のソースから `tools/hb-dump/build.sh` で作った実行ファイルを指す。
`SOFEL_FIXTURE_INPUTS` を設定すると、既存のテストは各ファイルの元になった合成フォントを書き出す。
書き出す入力には、対応する参照出力と同じファイル名を付ける。

```sh
export HB_DUMP="$PWD/target/hb-dump/hb-dump"
export SOFEL_FIXTURE_INPUTS=$(mktemp -d)
export SOFEL_FIXTURE_OUTPUTS=$(mktemp -d)
cargo test -p sofel-subset --test cff2 --test cff2_shared_global --test instance
```

各節のコマンドは、再生成した参照出力を `SOFEL_FIXTURE_OUTPUTS` に置く。
その後、同梱ファイルと `cmp` で比較できる。

## cff2-called-subrs-hb15.otf

入力は `tests/cff2.rs` の `cff2_called_subroutines_collect_nested_calls_and_renumber` が組む `bytes` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-called-subrs-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-called-subrs-hb15.otf" --gids=2
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-called-subrs-hb15.otf" "crates/sofel-subset/tests/data/cff2-called-subrs-hb15.otf"
```

## cff2-no-hint-empty-subr-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_removes_subroutines_containing_only_stems` が組む `bytes` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-no-hint-empty-subr-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-empty-subr-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-empty-subr-hb15.otf" "crates/sofel-subset/tests/data/cff2-no-hint-empty-subr-hb15.otf"
```

## cff2-no-hint-caller-stems-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_removes_stem_args_supplied_by_caller` が組む `bytes` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-no-hint-caller-stems-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-caller-stems-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-caller-stems-hb15.otf" "crates/sofel-subset/tests/data/cff2-no-hint-caller-stems-hb15.otf"
```

## cff2-hintmask-hb15.otf

入力は `tests/cff2.rs` の `cff2_hintmask_bytes_that_look_like_calls_are_not_calls` が組む `bytes` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-hintmask-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-hintmask-hb15.otf" --gids=2
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-hintmask-hb15.otf" "crates/sofel-subset/tests/data/cff2-hintmask-hb15.otf"
```

## cff2-subr-stem-hb15.otf

入力は `tests/cff2.rs` の `cff2_mask_length_counts_stems_added_in_a_subroutine` が組む `bytes` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-subr-stem-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-subr-stem-hb15.otf" --gids=2
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-subr-stem-hb15.otf" "crates/sofel-subset/tests/data/cff2-subr-stem-hb15.otf"
```

## cff2-context-mask-hb15.otf

入力は `tests/cff2.rs` の `cff2_shared_subroutine_with_conflicting_mask_lengths_retains_original_bytes` が組む `bytes` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-context-mask-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-context-mask-hb15.otf" --gids=1,2
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-context-mask-hb15.otf" "crates/sofel-subset/tests/data/cff2-context-mask-hb15.otf"
```

## cff2-context-mask-no-hint-hb15.otf

入力は `tests/cff2.rs` の `cff2_shared_subroutine_with_conflicting_mask_lengths_retains_original_bytes` が組む `bytes` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-context-mask-no-hint-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-context-mask-no-hint-hb15.otf" --gids=1,2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-context-mask-no-hint-hb15.otf" "crates/sofel-subset/tests/data/cff2-context-mask-no-hint-hb15.otf"
```

## cff2-context-mask-reversed-no-hint-hb15.otf

入力は `tests/cff2.rs` の `cff2_shared_subroutine_with_conflicting_mask_lengths_retains_original_bytes` が組む `reversed` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-context-mask-reversed-no-hint-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-context-mask-reversed-no-hint-hb15.otf" --gids=1,2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-context-mask-reversed-no-hint-hb15.otf" "crates/sofel-subset/tests/data/cff2-context-mask-reversed-no-hint-hb15.otf"
```

## adr0061-cff2-partial-context-hb15.otf

入力は `tests/cff2.rs` の `adr0061_i1_cff2_subroutine_context_with_partial_axis_matches_harfbuzz` が組む `bytes` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/adr0061-cff2-partial-context-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/adr0061-cff2-partial-context-hb15.otf" --gids=1,2 --no-hinting --instance=wght=300:500:700
cmp "$SOFEL_FIXTURE_OUTPUTS/adr0061-cff2-partial-context-hb15.otf" "crates/sofel-subset/tests/data/adr0061-cff2-partial-context-hb15.otf"
```

## cff2-no-hint-stems-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_removes_used_glyph_and_subroutine_stems` が組む `bytes` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-no-hint-stems-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-stems-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-stems-hb15.otf" "crates/sofel-subset/tests/data/cff2-no-hint-stems-hb15.otf"
```

## cff2-no-hint-local-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_keeps_only_used_local_subroutine_without_hints` が組む `bytes` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-no-hint-local-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-local-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-local-hb15.otf" "crates/sofel-subset/tests/data/cff2-no-hint-local-hb15.otf"
```

## adr0061-stat-endpoint-hb15.ttf

入力は `tests/instance.rs` の `adr0061_i1_subset_instance_endpoint_pin_filters_stat_without_variation_data` が組む `data` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/adr0061-stat-endpoint-hb15.ttf" --output-file="$SOFEL_FIXTURE_OUTPUTS/adr0061-stat-endpoint-hb15.ttf" --unicodes=41 --instance=wght=900
cmp "$SOFEL_FIXTURE_OUTPUTS/adr0061-stat-endpoint-hb15.ttf" "crates/sofel-subset/tests/data/adr0061-stat-endpoint-hb15.ttf"
```

## adr0061-stat-range-hb15.ttf

入力は `tests/instance.rs` の `adr0061_i1_subset_instance_range_filters_stat_without_variation_data` が組む `data` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/adr0061-stat-range-hb15.ttf" --output-file="$SOFEL_FIXTURE_OUTPUTS/adr0061-stat-range-hb15.ttf" --unicodes=41 --instance=wght=300:400:500
cmp "$SOFEL_FIXTURE_OUTPUTS/adr0061-stat-range-hb15.ttf" "crates/sofel-subset/tests/data/adr0061-stat-range-hb15.ttf"
```

## adr0061-stat-all-hb15.ttf

入力は `tests/instance.rs` の `adr0061_i1_subset_instance_all_axes_pinned_filters_stat_without_variation_data` が組む `data` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/adr0061-stat-all-hb15.ttf" --output-file="$SOFEL_FIXTURE_OUTPUTS/adr0061-stat-all-hb15.ttf" --unicodes=41 --instance=wght=900,wdth=drop
cmp "$SOFEL_FIXTURE_OUTPUTS/adr0061-stat-all-hb15.ttf" "crates/sofel-subset/tests/data/adr0061-stat-all-hb15.ttf"
```

## adr0061-stat-avar-hb15.ttf

入力は `tests/instance.rs` の `adr0061_i1_subset_instance_default_pin_keeps_avar_and_filters_stat` が組む `data` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/adr0061-stat-avar-hb15.ttf" --output-file="$SOFEL_FIXTURE_OUTPUTS/adr0061-stat-avar-hb15.ttf" --unicodes=41 --instance=wdth=drop
cmp "$SOFEL_FIXTURE_OUTPUTS/adr0061-stat-avar-hb15.ttf" "crates/sofel-subset/tests/data/adr0061-stat-avar-hb15.ttf"
```

## adr0061-avar-pin-hb15.ttf

入力は `tests/instance.rs` の `adr0061_i1_subset_instance_nondefault_pin_with_identity_avar` が組む `data` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/adr0061-avar-pin-hb15.ttf" --output-file="$SOFEL_FIXTURE_OUTPUTS/adr0061-avar-pin-hb15.ttf" --unicodes=41 --instance=wght=900
cmp "$SOFEL_FIXTURE_OUTPUTS/adr0061-avar-pin-hb15.ttf" "crates/sofel-subset/tests/data/adr0061-avar-pin-hb15.ttf"
```

## adr0061-avar-range-hb15.ttf

入力は `tests/instance.rs` の `adr0061_i1_subset_instance_range_with_identity_avar` が組む `data` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/adr0061-avar-range-hb15.ttf" --output-file="$SOFEL_FIXTURE_OUTPUTS/adr0061-avar-range-hb15.ttf" --unicodes=41 --instance=wght=300:400:500
cmp "$SOFEL_FIXTURE_OUTPUTS/adr0061-avar-range-hb15.ttf" "crates/sofel-subset/tests/data/adr0061-avar-range-hb15.ttf"
```

## adr0061-avar-one-sided-hb15.ttf

入力は `tests/instance.rs` の `adr0061_i1_subset_instance_one_sided_range_with_identity_avar` が組む `data` である。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/adr0061-avar-one-sided-hb15.ttf" --output-file="$SOFEL_FIXTURE_OUTPUTS/adr0061-avar-one-sided-hb15.ttf" --unicodes=41 --instance=wght=400:400:500
cmp "$SOFEL_FIXTURE_OUTPUTS/adr0061-avar-one-sided-hb15.ttf" "crates/sofel-subset/tests/data/adr0061-avar-one-sided-hb15.ttf"
```


## cff2-mask-or-stem-hb15.otf

入力は `tests/cff2.rs` の `cff2_shared_subroutine_with_conflicting_mask_lengths_retains_original_bytes` の `mask_or_stem` である。
共有サブルーチンの `hintmask` に続くバイトを、一方のグリフはマスク、もう一方は `hstem` として読む。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-mask-or-stem-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-mask-or-stem-hb15.otf" --gids=1,2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-mask-or-stem-hb15.otf" "crates/sofel-subset/tests/data/cff2-mask-or-stem-hb15.otf"
```

## cff2-stem-or-mask-hb15.otf

入力は `tests/cff2.rs` の `cff2_shared_subroutine_with_conflicting_mask_lengths_retains_original_bytes` の `stem_or_mask` である。
共有サブルーチンの `hintmask` に続くバイトを、一方のグリフはマスク、もう一方は `hstem` として読む。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-stem-or-mask-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-stem-or-mask-hb15.otf" --gids=1,2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-stem-or-mask-hb15.otf" "crates/sofel-subset/tests/data/cff2-stem-or-mask-hb15.otf"
```

## cff2-context-stem-path-hb15.otf

入力は `tests/cff2.rs` の `cff2_contextual_hint_commands_keep_the_shared_path` の `reverse == false` の `bytes` である。
共有サブルーチンの `hintmask` に続くバイトを、一方のグリフはマスク、もう一方は `hstem` として読む。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-context-stem-path-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-context-stem-path-hb15.otf" --gids=1,2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-context-stem-path-hb15.otf" "crates/sofel-subset/tests/data/cff2-context-stem-path-hb15.otf"
```

## cff2-context-mask-path-hb15.otf

入力は `tests/cff2.rs` の `cff2_contextual_hint_commands_keep_the_shared_path` の `reverse == true` の `bytes` である。
共有サブルーチンの `hintmask` に続くバイトを、一方のグリフはマスク、もう一方は `hstem` として読む。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-context-mask-path-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-context-mask-path-hb15.otf" --gids=1,2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-context-mask-path-hb15.otf" "crates/sofel-subset/tests/data/cff2-context-mask-path-hb15.otf"
```

## cff2-no-hint-blended-path-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_preserves_blended_paths_in_subroutines` が組む `bytes` である。CFF2 サブルーチンの `blend` を含む命令列に `NO_HINTING` を適用する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-no-hint-blended-path-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-blended-path-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-blended-path-hb15.otf" "crates/sofel-subset/tests/data/cff2-no-hint-blended-path-hb15.otf"
```

## cff2-no-hint-blended-stem-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_removes_blended_stems_in_subroutines` が組む `bytes` である。CFF2 サブルーチンの `blend` を含む命令列に `NO_HINTING` を適用する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-no-hint-blended-stem-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-blended-stem-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-blended-stem-hb15.otf" "crates/sofel-subset/tests/data/cff2-no-hint-blended-stem-hb15.otf"
```

## cff2-no-hint-dotsection-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_removes_dotsection_before_and_after_paths` が組む `bytes` である。描画命令の前後に引数のない `dotsection` を置いた CFF2 サブルーチンに `NO_HINTING` を適用する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-no-hint-dotsection-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-dotsection-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-dotsection-hb15.otf" "crates/sofel-subset/tests/data/cff2-no-hint-dotsection-hb15.otf"
```

## cff2-no-hint-context-call-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_rewrites_nested_calls_after_contextual_masks` の `reverse == false` が組む `bytes` である。マスク長が異なる二つの呼び出しと、そのサブルーチン内からの追加呼び出しを持つ。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-no-hint-context-call-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-context-call-hb15.otf" --gids=1,2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-context-call-hb15.otf" "crates/sofel-subset/tests/data/cff2-no-hint-context-call-hb15.otf"
```

## cff2-no-hint-context-call-reverse-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_rewrites_nested_calls_after_contextual_masks` の `reverse == true` が組む `bytes` である。マスク長が異なる二つの呼び出しと、そのサブルーチン内からの追加呼び出しを持つ。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-no-hint-context-call-reverse-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-context-call-reverse-hb15.otf" --gids=1,2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-context-call-reverse-hb15.otf" "crates/sofel-subset/tests/data/cff2-no-hint-context-call-reverse-hb15.otf"
```

## cff2-no-hint-vsindex-path-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_keeps_vsindex_for_blended_subroutine_paths` が組む `bytes` である。グリフに `vsindex`、サブルーチンの描画に `blend` があり、除去する stem はない。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-no-hint-vsindex-path-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-vsindex-path-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-no-hint-vsindex-path-hb15.otf" "crates/sofel-subset/tests/data/cff2-no-hint-vsindex-path-hb15.otf"
```

## cff2-vsindex-before-stem-hb15.otf

入力は `tests/cff2.rs` の `cff2_vsindex_hint_prefix_relocation_matches_harfbuzz` の prefix_first == true のケースが組む `bytes` である。`vsindex` と stem を持つ CFF2 をヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-vsindex-before-stem-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-vsindex-before-stem-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-vsindex-before-stem-hb15.otf" "crates/sofel-subset/tests/data/cff2-vsindex-before-stem-hb15.otf"
```

## cff2-vsindex-after-stem-hb15.otf

入力は `tests/cff2.rs` の `cff2_vsindex_hint_prefix_relocation_matches_harfbuzz` の prefix_first == false のケースが組む `bytes` である。`vsindex` と stem を持つ CFF2 をヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-vsindex-after-stem-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-vsindex-after-stem-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-vsindex-after-stem-hb15.otf" "crates/sofel-subset/tests/data/cff2-vsindex-after-stem-hb15.otf"
```

## cff2-vsindex-subr-path-hb15.otf

入力は `tests/cff2.rs` の `cff2_vsindex_in_subroutine_moves_prefix_to_glyph` の keep_path == true のケースが組む `bytes` である。`vsindex` と stem を持つ CFF2 をヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-vsindex-subr-path-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-vsindex-subr-path-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-vsindex-subr-path-hb15.otf" "crates/sofel-subset/tests/data/cff2-vsindex-subr-path-hb15.otf"
```

## cff2-vsindex-subr-empty-hb15.otf

入力は `tests/cff2.rs` の `cff2_vsindex_in_subroutine_moves_prefix_to_glyph` の keep_path == false のケースが組む `bytes` である。`vsindex` と stem を持つ CFF2 をヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-vsindex-subr-empty-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-vsindex-subr-empty-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-vsindex-subr-empty-hb15.otf" "crates/sofel-subset/tests/data/cff2-vsindex-subr-empty-hb15.otf"
```

## cff2-vsindex-nonzero-hb15.otf

入力は `tests/cff2.rs` の `cff2_nonzero_vsindex_prefix_matches_harfbuzz` の vsindex が1 のケースが組む `bytes` である。`vsindex` と stem を持つ CFF2 をヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-vsindex-nonzero-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-vsindex-nonzero-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-vsindex-nonzero-hb15.otf" "crates/sofel-subset/tests/data/cff2-vsindex-nonzero-hb15.otf"
```

## cff2-vsindex-caller-path-hb15.otf

入力は `tests/cff2.rs` の `cff2_path_only_vsindex_accepts_caller_argument` が組む `bytes` である。グリフが渡した引数をサブルーチンの `vsindex` が読み、stem を除かずに描画の `blend` に使う。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-vsindex-caller-path-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-vsindex-caller-path-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-vsindex-caller-path-hb15.otf" "crates/sofel-subset/tests/data/cff2-vsindex-caller-path-hb15.otf"
```

## cff2-hint-args-nested-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_removes_arguments_through_nested_calls` が組む `bytes` である。入れ子の呼び出しを介して stem 引数を渡すケースをヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-hint-args-nested-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-hint-args-nested-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-hint-args-nested-hb15.otf" "crates/sofel-subset/tests/data/cff2-hint-args-nested-hb15.otf"
```

## cff2-hint-args-blend-path-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_removes_forwarded_blends_but_keeps_path` が組む `bytes` である。global と local の呼び出しを介して blend の結果を渡し、描画命令を残すケースをヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-hint-args-blend-path-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-hint-args-blend-path-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-hint-args-blend-path-hb15.otf" "crates/sofel-subset/tests/data/cff2-hint-args-blend-path-hb15.otf"
```

## cff2-hint-args-leaf-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_removes_arguments_split_across_callers` が組む `bytes` である。leaf_args == true で、グリフと末端サブルーチンが引数を積むケースをヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-hint-args-leaf-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-hint-args-leaf-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-hint-args-leaf-hb15.otf" "crates/sofel-subset/tests/data/cff2-hint-args-leaf-hb15.otf"
```

## cff2-hint-args-middle-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_removes_arguments_split_across_callers` が組む `bytes` である。leaf_args == false で、グリフと中間サブルーチンが引数を積むケースをヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-hint-args-middle-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-hint-args-middle-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-hint-args-middle-hb15.otf" "crates/sofel-subset/tests/data/cff2-hint-args-middle-hb15.otf"
```

## cff2-hint-vsindex-caller-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_relocates_vsindex_argument_from_caller` が組む `bytes` である。グリフからサブルーチン先頭の vsindex へ引数を渡すケースをヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-hint-vsindex-caller-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-hint-vsindex-caller-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-hint-vsindex-caller-hb15.otf" "crates/sofel-subset/tests/data/cff2-hint-vsindex-caller-hb15.otf"
```

## cff2-hint-args-return-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_removes_hints_after_argument_return` が組む `bytes` である。
サブルーチンの戻り値を stem 引数に使うケースをヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-hint-args-return-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-hint-args-return-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-hint-args-return-hb15.otf" "crates/sofel-subset/tests/data/cff2-hint-args-return-hb15.otf"
```

## cff2-hint-blend-operands-caller-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_removes_blend_operands_from_caller` が組む `bytes` である。
呼び出し元の値を blend の引数として使い、その結果を stem に渡すケースをヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-hint-blend-operands-caller-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-hint-blend-operands-caller-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-hint-blend-operands-caller-hb15.otf" "crates/sofel-subset/tests/data/cff2-hint-blend-operands-caller-hb15.otf"
```

## cff2-dotsection-args-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_handles_dotsection_arguments` が組む `bytes` である。
引数のある dotsection の後に stem を宣言するケースをヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-dotsection-args-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-dotsection-args-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-dotsection-args-hb15.otf" "crates/sofel-subset/tests/data/cff2-dotsection-args-hb15.otf"
```

## cff2-stem-after-moveto-hb15.otf

入力は `tests/cff2.rs` の `cff2_no_hinting_handles_stems_after_moveto` が組む `bytes` である。
moveto の後に stem を宣言するケースをヒントなしでサブセット化する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-stem-after-moveto-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-stem-after-moveto-hb15.otf" --gids=2 --no-hinting
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-stem-after-moveto-hb15.otf" "crates/sofel-subset/tests/data/cff2-stem-after-moveto-hb15.otf"
```

## cff2-flat-vsindex-hb15.otf

入力は `tests/cff2_flat_vsindex.rs` の `cff2_flatten_preserves_explicit_vsindex_without_blend` が組む。軸を指定しない CFF2 で、サブルーチンを呼ばず、描画の `blend` も使わないが、明示的な `vsindex 0` を持つ。固定した HarfBuzz 15.1.0 の出力を比較する。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-flat-vsindex-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-flat-vsindex-hb15.otf" --gids=1
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-flat-vsindex-hb15.otf" "crates/sofel-subset/tests/data/cff2-flat-vsindex-hb15.otf"
```

## cff2-flat-vsindex-desubr-hb15.otf

入力は `cff2-flat-vsindex-hb15.otf` と同じである。`DESUBROUTINIZE` を指定すると、固定した HarfBuzz 15.1.0 は描画の `blend` に使わない `vsindex` を除く。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-flat-vsindex-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-flat-vsindex-desubr-hb15.otf" --gids=1 --desubroutinize
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-flat-vsindex-desubr-hb15.otf" "crates/sofel-subset/tests/data/cff2-flat-vsindex-desubr-hb15.otf"
```

## cff2-shared-global-hb15.otf

`tests/cff2_shared_global.rs` の `adr0102_a1_export_reference_inputs` が組む入力で、Font DICT 0 と 1 の local 再採番が異なる。gid 1 と 2 を残すと、gid 2 の第2輪郭が元の `(20,10)` から `(30,10)` へ変わる。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-shared-global-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-shared-global-hb15.otf" --gids=1,2
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-shared-global-hb15.otf" "crates/sofel-subset/tests/data/cff2-shared-global-hb15.otf"
```

## cff2-shared-global-gid2-hb15.otf

同じ入力の gid 2 だけを残した出力は、global から存在しない local を呼ぶ。検証器のエラーと sofel の `Malformed` を確かめるために置く。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-shared-global-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-shared-global-gid2-hb15.otf" --gids=2
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-shared-global-gid2-hb15.otf" "crates/sofel-subset/tests/data/cff2-shared-global-gid2-hb15.otf"
```

## cff2-shared-global-equal-hb15.otf

両 Font DICT の local 再採番が同じ入力では、元の輪郭を保つ。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-shared-global-equal-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-shared-global-equal-hb15.otf" --gids=1,2
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-shared-global-equal-hb15.otf" "crates/sofel-subset/tests/data/cff2-shared-global-equal-hb15.otf"
```

## cff2-varstore-none-hb15.otf

`tests/cff2_shared_global.rs` が組む、VariationStore の参照が0件の入力。未使用のデータ行と領域も残す。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-varstore-none-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-varstore-none-hb15.otf" --gids=1
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-varstore-none-hb15.otf" "crates/sofel-subset/tests/data/cff2-varstore-none-hb15.otf"
```

## cff2-varstore-partial-hb15.otf

VariationStore の最初の行だけを描画の `blend` が使う入力で、明示的な `vsindex` も残す。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-varstore-partial-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-varstore-partial-hb15.otf" --gids=1
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-varstore-partial-hb15.otf" "crates/sofel-subset/tests/data/cff2-varstore-partial-hb15.otf"
```

## cff2-varstore-all-hb15.otf

VariationStore の2行を別のグリフが使う入力。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-varstore-all-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-varstore-all-hb15.otf" --gids=1,2
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-varstore-all-hb15.otf" "crates/sofel-subset/tests/data/cff2-varstore-all-hb15.otf"
```

## cff2-varstore-unused-vsindex-hb15.otf

明示的な `vsindex` を持つが、描画の `blend` を使わない入力。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-varstore-unused-vsindex-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-varstore-unused-vsindex-hb15.otf" --gids=1
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-varstore-unused-vsindex-hb15.otf" "crates/sofel-subset/tests/data/cff2-varstore-unused-vsindex-hb15.otf"
```

## cff2-varstore-partial-desubr-hb15.otf

`partial` と同じ入力で `DESUBROUTINIZE` を指定する。描画の `blend` に必要な `vsindex` は残る。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-varstore-partial-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-varstore-partial-desubr-hb15.otf" --gids=1 --desubroutinize
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-varstore-partial-desubr-hb15.otf" "crates/sofel-subset/tests/data/cff2-varstore-partial-desubr-hb15.otf"
```

## cff2-varstore-unused-vsindex-desubr-hb15.otf

`unused-vsindex` と同じ入力で `DESUBROUTINIZE` を指定する。使わない `vsindex` は省く。

```sh
"$HB_DUMP" --subset --font-file="$SOFEL_FIXTURE_INPUTS/cff2-varstore-unused-vsindex-hb15.otf" --output-file="$SOFEL_FIXTURE_OUTPUTS/cff2-varstore-unused-vsindex-desubr-hb15.otf" --gids=1 --desubroutinize
cmp "$SOFEL_FIXTURE_OUTPUTS/cff2-varstore-unused-vsindex-desubr-hb15.otf" "crates/sofel-subset/tests/data/cff2-varstore-unused-vsindex-desubr-hb15.otf"
```
