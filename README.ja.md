# netdistill

[English](README.md) | 日本語

**`show ip route` の5万行をそのまま LLM に貼るのはやめよう。**

ネットワーク機器の CLI 出力は人間の目視用にできています。固定幅パディング、繰り返されるヘッダ、そして 99% が「正常です」としか言っていない行の羅列。これを LLM に食わせるとコンテキストウィンドウがパディングで埋まります。フルルートの `show route vrf all` は数百万トークン相当で、そもそも入りません。

`netdistill` は `show` 出力や config を「LLM がネットワークについて推論するのに本当に必要な形」に蒸留するシングルバイナリ CLI です。

- **異常が先頭** — ダウンしたセッション、エラーカウンタ、フラップ、hidden route が最初に出る
- **ダンプではなく集計** — BGP 5万経路は VRF×プロトコルのマトリクスと next-hop ランキングになる
- **省略の明示** — 省いたものは必ず明記（「healthy な Established ピア495件を省略」）。モデルが「書いてない＝正常」と誤解しない
- **トークンレシート** — 出力の先頭に `input ~X tok → output ~Y tok` が付き、削減量が見える

実測（`examples/gen_big_inputs.sh` による現実的な合成入力）:

| 入力 | 生 (推定トークン) | 蒸留後 | 削減率 | 処理時間 |
|---|---|---|---|---|
| `show ip route vrf all` 5万経路 | 約729,000 | 約164 | 99.98% | 20 ms |
| `show bgp summary` 500ピア(うち3異常) | 約12,600 | 約222 | 98% | <10 ms |

異常な3ピアは必ず残り、正常な497ピアは1行に畳まれます。

> ⚠️ **プロトタイプ。** パーサはヒューリスティックで、全フォーマット網羅ではありません。IOS/IOS-XE・FRR・Junos 系の出力形状で確認済み。未知のフォーマットは失敗せず generic 圧縮にフォールバックします。

## 現在理解できる入力

| 入力 | 判定 | 蒸留内容 |
|---|---|---|
| `show bgp summary` (IOS / FRR / EOS系, Junos) | `bgp-summary` | ピア状態の集計、経路数合計、上位ピア。異常/注目ピアのみ列挙 |
| `show ip route [vrf all]` (IOS/XR), `show route` (Junos) | `route-table` | VRF×プロトコル表、デフォルトルート有無、上位 next-hop、hidden route 警告 |
| `show interfaces` (IOS系) | `interfaces` | up/down 集計。エラー・ドロップ・想定外 down の IF のみ列挙 |
| running/startup config (IOS系) | `config` | インベントリ要約: IF 表、BGP ネイバー、VRF、ポリシー数。シークレットは出力しない |
| その他すべて | `generic` | 準ロスレス圧縮: パディング→タブ、重複行→`[xN]`、空行圧縮 |

**ターミナルセッション丸ごと**を貼ると、プロンプト行（`router1# show ...`）で分割してコマンドごとに蒸留します。

## インストール

Rust 1.75+ が必要です。

```bash
cargo install --path .
# または: cargo build --release   → target/release/netdistill
```

## 使い方

```bash
netdistill session.log                 # ファイル指定（複数可）
ssh router1 'show ip route vrf all' | netdistill     # stdin でも

netdistill --full show_bgp.txt         # 全行残す（構造化はされる）
netdistill --budget 800 session.log    # 約800トークンに収まるよう表を削る
netdistill --json show_bgp.txt         # パイプライン向け JSON
```

エージェント/パイプラインでの典型: コマンド実行 → `netdistill` に通す → ダイジェストをモデルへ。省略された詳細が必要になったら、そのコマンドだけ `--full` で再実行。省略ノートが「何を再要求すべきか」をモデルに教えます。

## 組み込み方法

### MCP サーバ（Claude Code などのエージェント向け）

```bash
netdistill mcp        # stdio 上の MCP。distill ツールを1つ公開
```

Claude Code への登録:

```bash
claude mcp add netdistill -- netdistill mcp
```

エージェント側には `distill` ツール（必須 `input`、任意 `full` / `budget` / `format`）が生え、ルータ出力をコンテキストに流し込む前に圧縮できます。

### HTTP API（任意の言語から）

```bash
netdistill serve                       # http://127.0.0.1:8321
netdistill serve --listen 0.0.0.0:8321 # 外部公開は自己責任 — 認証は内蔵していません
```

```bash
# 生テキスト + クエリパラメータ
ssh r1 'show ip route vrf all' | curl -s --data-binary @- 'localhost:8321/v1/distill?budget=800'

# JSON エンベロープで投げて JSON で受ける — Python でも Node でも Go でも
curl -s -H 'content-type: application/json' \
     -d '{"input": "...", "format": "json"}' localhost:8321/v1/distill
```

`GET /healthz` で死活監視、`GET /` で使い方。JSON レスポンスには `docs[]`（alerts / summary / tables が構造のまま）とトークン推定が入るので、「`alerts` が空でなければ LLM を起こす」という**前段フィルタ**にも使えます。

## 設計原則

1. **LLM の仕事は診断。ログではなく証拠を渡す。** 異常と集計は、生出力の持つ診断情報のほぼ全てを数百分の一のトークンで運ぶ。
2. **省略は明示する。** 黙った切り詰めはモデルに健全性を幻覚させる。省略は必ずインラインで宣言。
3. **構造は散文に勝る。** パース済みのコンパクトな表は、固定幅の表をモデルに目視させるより小さく、かつ正確。
4. **優雅に劣化する。** 未知フォーマットはエラーではなく generic 圧縮へ。誤パースでも「より小さく、嘘のない入力」であること。

すべての判定基準・しきい値は [docs/how-it-works.ja.md](docs/how-it-works.ja.md) に詳解があります。

## ライセンス

MIT
