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
| `show bgp summary` (IOS / IOS-XR / FRR / NX-OS / EOS / Huawei VRP / Junos) | `bgp-summary` | ピア状態の集計、経路数合計、上位ピア。異常/注目ピアのみ列挙 |
| `show ip bgp` フルテーブル (IOS / FRR) | `bgp-table` | networks/paths/best 集計、origin AS ヒストグラム、AS パス長統計、suppressed/damped/RIB-failure 警告。prefix は再出力しない |
| `show ip route [vrf all]` (IOS/XR/NX-OS), `show route` (Junos) | `route-table` | VRF×プロトコル表、デフォルトルート有無、上位 next-hop、hidden route 警告 |
| `show interfaces` (IOS系) | `interfaces` | up/down 集計。エラー・ドロップ・想定外 down の IF のみ列挙 |
| `show ip ospf neighbor` (IOS / FRR) | `ospf-neighbors` | IF別の状態集計。FULL でない(スタックした)隣接のみ列挙。2WAY/DROTHER は正常扱い |
| `show isis neighbors` (IOS / FRR) | `isis-neighbors` | レベル/IF 別集計。UP でない隣接のみ列挙 |
| `show lldp neighbors` (IOS系) | `lldp-neighbors` | インベントリは全行保持(トポロジ情報)、パディングのみ除去。宣言件数とパース行数の不一致を警告 |
| running/startup config (IOS系) | `config` | インベントリ要約: IF 表、BGP ネイバー、VRF、ポリシー数。シークレットは出力しない |
| `show configuration` (Junos、set形式**と**階層形式の両方) | `junos-config` | 同様のインベントリ要約。両形式を単一の抽出器に正規化。シークレットは出力しない |
| その他すべて | `generic` | 準ロスレス圧縮: パディング→タブ、重複行→`[xN]`、空行圧縮 |

**ターミナルセッション丸ごと**を貼ると、プロンプト行（`router1# show ...`）で分割してコマンドごとに蒸留します。

## インストール

### バイナリ（推奨 — Rust 不要）

[Releases](https://github.com/pathvector-studio/netdistill/releases) から静的バイナリをダウンロードして PATH に置くだけです（Linux x86_64/aarch64 は musl 静的リンクでどのディストリでも動作、macOS は Intel/Apple Silicon 両対応）。ランタイムも VM もインタプリタも不要です。

### ソースから

Rust 1.75+ が必要です（ビルド時のみ）。

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
netdistill --kind route-table dump.txt # 自動判定を使わずパーサを強制指定
netdistill diff before.txt after.txt   # 2スナップショット間の変化だけ出す
netdistill diff a.txt b.txt --drift 0  # 微小なカウンタ変動も報告する
netdistill --section next-hops r1.txt  # 省略された表を1つだけ完全展開
```

エージェント/パイプラインでの典型: コマンド実行 → `netdistill` に通す → ダイジェストをモデルへ。省略された詳細が必要になったら、そのコマンドだけ `--full` で再実行。省略ノートが「何を再要求すべきか」をモデルに教えます。

### diff モード

`netdistill diff before.txt after.txt` は同一コマンドの2スナップショットを比較し、**変わったところだけ**を出します: `NEW:`/`RESOLVED:` のアラート、`old → new` 形式のサマリ変化、行単位の `Δ` 表（`+` 追加 / `-` 削除 / `~` 変更、セル単位で `old→new`）。同一の健全なスナップショット同士なら `no changes detected` の一行に畳まれます — 「メンテで何か壊れた？」への最安の回答です。内部では両側を省略なしでパースするので、普段は省略される healthy なエントリの変化も見逃しません。MCP の `diff` ツールと `POST /v1/diff` でも使えます。

カウンタノイズは既定で抑制されます: uptime の進行や 10% 未満の数値変動しかない行は（明示ノート付きで）省かれ、タイムスタンプだけが違うアラートは NEW/RESOLVED のペアとしてバタつきません。`--drift <pct>` で調整、`--drift 0` で全報告。

### ドリルダウン

ダイジェストに「495行省略」と出たら、`--section <名前>` でその表だけを省略なしで再出力できます: `netdistill --section "next-hops" routes.txt`。表タイトルの部分一致（大文字小文字無視）で、ヒットしなければ利用可能なタイトル一覧を返します。MCP `distill` の引数と HTTP の `?section=` でも使えるので、エージェントは「まずダイジェスト → 必要な表だけ掘る」という2段階読みができます。

### serve の認証

`netdistill serve --token <secret>`（または環境変数 `NETDISTILL_TOKEN`）で、`/healthz` 以外の全エンドポイントに `Authorization: Bearer <secret>` か `?token=` を要求します。トークン未設定なら従来通りオープン — その場合は localhost に留めてください。

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
