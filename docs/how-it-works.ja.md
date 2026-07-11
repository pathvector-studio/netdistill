# netdistill の仕組み — ロジック詳解

[English](how-it-works.md) | 日本語

netdistill が「73万トークンの `show ip route vrf all` を 164 トークンにする」ときに内部で何をしているかを、判断基準・しきい値まで含めて説明します。ソースの対応箇所を各節に付記しています。

## 全体パイプライン

```
入力テキスト
   │
   ▼
[1] セッション分割 (lib.rs::split_sessions)
   │   プロンプト行 "router1# show ..." でコマンドごとのチャンクに分割
   ▼
[2] 種別判定 (detect.rs::detect)           … チャンクごとに実行
   │   bgp-summary / route-table / interfaces / config / generic
   ▼
[3] パース＆蒸留 (parse/*.rs)
   │   構造化 → 集計 → 異常抽出 → 省略対象の決定
   ▼
[4] Doc モデル (model.rs)
   │   { headline, alerts[], summary[], tables[], notes[], body? }
   ▼
[5] レンダリング (render.rs)
   │   テキスト or JSON。--budget があれば収まるまで表を削る
   ▼
出力（先頭にトークンレシート）
```

中間表現 `Doc` がすべての中心です。どのパーサも「アラート」「集計」「表」「省略ノート」という同じ語彙に落とすため、レンダラと予算制御はパーサの種類を知らずに動きます。

## 設計原則（なぜこの形か）

1. **異常が先頭** — LLM にネットワーク出力を渡す目的はほぼ常に診断。診断信号の大半は「異常・変化・集計」に宿り、正常エントリの羅列にはalmost無い。
2. **省略は必ず明示** — 黙って切り詰めると LLM は「書いてない＝正常」と誤解する（＝健全性の幻覚）。省略には必ず「495 healthy peers omitted (--full to list all)」のようなインラインの宣言を付ける。これにより LLM は「不足している詳細を何と言って再要求すべきか」も分かる。
3. **構造は散文に勝る** — 固定幅テーブルを LLM に目視パースさせると行ズレ・数値取り違えが起きる。事前パースしたタブ区切り表は、小さい上に正確。
4. **優雅な劣化** — 未知フォーマットはエラーにせず generic 圧縮へ。誤検出しても「より小さく、嘘のない入力」に留まることを不変条件とする。

## [1] セッション分割 — `split_sessions`

ターミナルログを丸ごと貼れるようにするための前処理。

**プロンプト行の判定** (`prompt_command`): 行を `#` または `>` で1回分割し、以下すべてを満たせばプロンプトとみなす:

- 区切り文字より前（ホスト名部）が非空・64文字以下・**空白を含まない**
- 区切り文字より後のコマンドが `show ` または `display ` で始まる

この条件により、Junos の経路エントリ内 `> to 10.1.1.2 via ...`（前部が空）や、`#` を含む通常行（前部に空白がある）を誤検出しません。

分割後、プロンプトが1つでも見つかった場合は、**ラベルなしで非空行3行未満の断片**（バナーやプロンプト残骸）を捨てます。プロンプトが1つも無ければ入力全体を1チャンクとして扱います。検出したコマンド文字列は Doc の headline に `` `show ip bgp summary` — ... `` の形で埋め込まれます。

## [2] 種別判定 — `detect`

先頭200行だけを見る軽量ヒューリスティック。**順序が仕様**です（後述の理由で上から評価）:

| 優先 | 種別 | 判定条件 | 順序の理由 |
|---|---|---|---|
| 1 | `interfaces` | `"line protocol is"` を含む | この文字列は show interfaces 固有。config には現れない |
| 2 | `bgp-summary` | `MsgRcvd` / `State/PfxRcd` / (`InPkt`かつ`OutPkt`) / `BGP router identifier` のいずれか | ヘッダ行の列名はベンダ間で最も安定した指紋 |
| 3 | `route-table` | `Routing Table:` / `Gateway of last resort` / `variably subnetted` / 行頭`Codes:` / Junos RIB ヘッダ | config も `ip route` を含むため、route table 固有の見出しで先に確定させる |
| 4 | `config` | 行頭 `interface ` があり、かつ (`!` だけの行が3つ以上 or 行頭 `hostname `) | `interface` ブロック＋IOS 特有の `!` 区切りの組み合わせ |
| 5 | `generic` | 上記いずれにも該当しない | フォールバック。エラーにはしない |

Junos RIB ヘッダの判定 (`junos_rib_header`) は「`:` で分割 → 名前部が `inet` を含み空白なし → 残りに `destinations` を含む」で、`inet.0:` も `CUST-A.inet6.0:` も拾います。

**既知の限界**: `show ip bgp`（フルテーブル）は先頭に `BGP router identifier` を含むため bgp-summary と誤判定されます（v0.2 で対応予定）。誤判定してもピア行として解釈できない行は全部読み飛ばされるので、出力は「ピア0件」となり嘘はつきません（原則4）。

## [3] パーサ詳細

### bgp-summary (`parse/bgp_summary.rs`)

**方言の自動切替**: ヘッダ行に `InPkt`+`OutPkt` を見たら Junos モード、`Neighbor`+`MsgRcvd` を見たら IOS/FRR モード。行単位で切り替わるのでモード宣言は不要。

**行の採用基準**: 先頭トークンが IP アドレスに見える行だけをピア行として読む。`is_addr` は「`.` を含み数字と`.`のみ」（v4）または「`:` を含み16進と`:.`のみ」（v6）。FRR の dynamic peer 接頭辞 `*` は剥がします。

**IOS/FRR 行** (10列以上): `Neighbor V AS MsgRcvd MsgSent TblVer InQ OutQ Up/Down State/PfxRcd` の位置固定で読み、**第10列が数値なら Established＋受信prefix数、非数値なら状態文字列**。`Idle (Admin)` のような2トークン状態は、次列が `(` で始まる場合のみ結合します（FRR の PfxSnt/Desc 列を状態に巻き込まないため）。

**Junos 行** (8列以上): 最終列が `数字/数字/...` 形式なら Established（先頭の数値 = Active prefix 数）。`Establ` などの単語なら状態として採用し `Estab*` → `Established` に正規化。Up/Down は「第7列〜最終列の手前」を結合（`12w4d 2:03:11` のような空白入りに対応）。直後のインデント行 `inet.0: 1822/1830/1830/0` は**直前のピアへの継続行**として prefix 数を補完します。

**アラート規則**（この順で評価、上が優先）:

| 条件 | アラート |
|---|---|
| state ≠ Established | `session <state> — up/down <時間>` |
| Established かつ pfx == 0 | `Established but 0 prefixes received` （up 状態でも実質死んでいるセッション） |
| Established かつ Up/Down が `00:` で始まる | `session reset recently` （1時間以内の再確立＝直近フラップ） |
| (Junos) Flaps ≥ 3 | `<n> flaps` （独立に追加。上と重複可） |

**集計**: 状態別ピア数 / 受信prefix総数 / prefix数上位3ピア（AS・件数・アドレス）/ AF・VRFセクションが複数あればその一覧。

**省略規則**: 表に載るのはアラート対象ピアのみ。healthy な Established ピアは件数だけを省略ノートに残す。全ピア healthy なら表自体を省略し、その旨をノートに書く。`--full` で全ピア表に切替。

### route-table (`parse/route.rs`)

**テーブル(VRF)境界**: `Routing Table: <name>`（IOS）、`VRF: <name>`（IOS-XR）、Junos RIB ヘッダ（`inet.0: N destinations, M routes (... K hidden)`）で現在のテーブルを切替。Junos ヘッダからは hidden 数も取得。

**IOS 系の経路行**: **列0から始まる行**のみ経路候補（IOS は経路行を左端に、`10.0.0.0/8 is variably subnetted...` のような親見出しやECMP継続行をインデントして出力するため、この1条件でほぼ分離できる）。先頭トークンがコード（3文字以下・英字と`*%+`のみ）で、2番目トークンが既知サブコード（`IA E1 E2 EX N1 N2 L1 L2 su ...`）ならそれも読み飛ばし、次のトークンを prefix とする。`subnetted` を含む行は見出しなので除外。

- プロトコル正規化: `C→connected, L→local, S→static, O→ospf, B→bgp, D→eigrp, R→rip, i→isis, K→kernel`。`O IA` `O E2` はすべて ospf に、`S*` は static に集約（`*` は candidate default マーカーなので剥がす）
- next-hop: `via ` の直後トークン。`is directly connected, Gi0/0` は `direct:<IF>` として記録
- インデント行のうち `[` で始まり ` via ` を含むものは **ECMP/折返し継続行**として next-hop カウントのみ加算（経路数は増やさない）
- デフォルトルート: `Gateway of last resort is <X>`（`not set` なら無し）と `0.0.0.0/0` / `::/0` エントリの両方から検出

**Junos の経路行**: 先頭トークンが prefix で第2トークンが `*[Proto/pref]` 形式。プロトコルはブラケット内から取得。続く `> to <nh> via <if>` 行で next-hop を数え、直前エントリがデフォルトルートなら default_via をその nh で確定。

**アラート**: 経路数0のテーブル / hidden route を持つテーブル（Junos。ポリシーで隠された経路は障害調査で最初に見るべき場所）。

**集計と表**: プロトコル別総数（降順）/ デフォルトルートが無いテーブルの列挙（全テーブルに無ければ「no default route in any table」）/ **VRF×プロトコルのマトリクス**（列は上位6プロトコル＋other、各行に default_via）/ **next-hop ランキング上位8**（経路がどのゲートウェイに集中しているかは、障害の影響範囲を一目で伝える）。

**省略規則**: 個々の prefix は既定で出さない（これが最大の圧縮源。5万経路→数行）。ノートに「aggregated by table/protocol (--full to list every route)」と明示。

### interfaces (`parse/interfaces.rs`)

**ブロック開始**: 列0の行で ` is ` と `line protocol is` を両方含むもの。IF名は ` is ` の前、admin down は `administratively down` の有無、oper up は `line protocol is ` の直後が `up` か。

**カウンタ抽出**: 以後のインデント行から、キーワードの**直前の数値**を拾う方式（`14411 input errors, 14200 CRC, ...` → `input errors` の前の 14411）。`Total output drops:` のみ直後の数値。`Description:` / `Internet address is` も回収。

**アラート**: admin-down でないのに line protocol down（意図しない断）/ input errors > 0（CRC 数併記。L1品質問題）/ output errors > 0 / output drops > 0（輻輳）。しきい値はすべて「0より大きいか」— カウンタの絶対値の解釈は LLM 側に委ねる。

**省略規則**: 表はアラート対象IFのみ。healthy は件数のみ。

### config (`parse/config.rs`)

IOS config の構造規約「ブロックの子行は空白1つでインデント」を使い、行送りでブロックを歩きます。

- `interface` ブロック: description / IP（`255.255.255.0` 形式のマスクは `count_ones()` の合計で `/24` に変換）/ vrf forwarding / shutdown / access vlan を回収
- `router bgp` ブロック: AS番号、`neighbor X remote-as/description/peer-group` をアドレスごとにマージ、address-family 一覧
- トップレベル1行もの: static route 数 / ACL・prefix-list・route-map の**名前数**（行数ではない）/ vrf definition / ntp・snmp・aaa・logging・username の存在と数
- 認識しなかったトップレベルブロックは先頭語を集めて「present but not summarized: crypto, line, ...」とノートに列挙 — **要約対象外の存在を隠さない**

**シークレット方針**: `username ... secret 9 $9$...` などの行は**構造ごと読み捨て**、値をどのフィールドにも write しない。ノートに「secrets/crypto/aaa details are not extracted」と明記。蒸留出力が SaaS の LLM に渡る前提のため、ここはヒューリスティックではなく設計で保証します。

**省略規則**: 何も設定されていない IF（IP・desc・vrf・vlan・shutdown いずれも無し）は表から省き、件数をノートへ。

### generic (`parse/generic.rs`)

パーサが無い入力の**準ロスレス圧縮**。3変換のみ:

1. **whitespace squeeze**: 行内の連続2スペース以上（＝固定幅の列パディング）をタブ1つに置換。先頭インデントはスペース1つに丸める。意味は変えず、トークンだけ減らす
2. **重複行の RLE**: 連続する同一行を `<行>  [x5]` に畳む（同一 syslog の連打などに効く）
3. **空行連打を1つに**

変換内容は必ずノートに宣言します（LLM が `[x5]` 記法を注釈として理解できるように）。

## [4] トークン見積り — `token.rs`

`ceil(文字数 × 2 / 7)` ≒ **3.5文字/トークン**。根拠: ネットワーク系テキストは IP アドレス（オクテットごとに分割されがち）・カウンタ・パディングだらけで、英文の目安（4文字/トークン）より確実に悪い。正確なトークナイザは持たない方針（依存ゼロ・全モデル非依存）で、出力では常に `~` と `estimates` を付けて見積りであることを明示します。v0.3 で feature flag による実トークナイザ計数を予定。

## [5] レンダリングと予算制御 — `render.rs`

**出力フォーマット**は LLM の読み取り精度を優先した固定構造:

```
# netdistill — input ~X tok → output ~Y tok (Z% saved; estimates)   ← レシート
## [種別] ヘッドライン
ALERTS (n): / ALERTS: none      ← 必ず最初。「none」も明示（無言と区別）
SUMMARY:
<表タイトル>:
ヘッダ\t区切り                  ← タブ区切り: 桁揃えパディングを排除
(省略ノート)                    ← 表の直下にインライン
NOTES:
```

**`--budget N` の充足アルゴリズム** (`shrink`): 描画→見積り→超過なら1ステップ縮小、のループ。縮小順序は情報価値の低い順:

1. **最大の表を半分に切る**（4行以上ある表のみ対象）。切った行数を省略ノートに追記（`... 12 rows trimmed for --budget`）
2. 表がすべて3行以下になったら、**最大の生ボディ（generic）を半分に**（8行以上のみ）。`[... N lines trimmed]` を付記
3. どちらも不可能になったら停止し、`[!] --budget could not be met; smallest faithful output shown` を付けて出力

**ALERTS と SUMMARY は絶対に削らない**のが不変条件です。予算がどれだけ小さくても「異常の列挙と全体像」だけは完全な形で残ります。半減ループなので計算量は O(log(行数)) 回の再描画で収まります。

## 圧縮率の構造

50,000経路 → 164トークン (99.98%) の内訳を分解すると:

| 手法 | 寄与 |
|---|---|
| 個別 prefix の省略（集計への置換） | 圧倒的大部分。5万行 → マトリクス2行 |
| 正常エントリの省略（異常のみ列挙） | ピア系で支配的。500行 → 3行 |
| パディング→タブ、ヘッダ非反復 | 生き残った行を各 2〜3 倍圧縮 |
| 省略の明示ノート | **負の寄与**（トークンを足す）が、幻覚防止のための意図的コスト |

つまり netdistill の本質は文字列圧縮ではなく「**LLM の診断に必要な統計量だけを残す不可逆圧縮 ＋ 何を落としたかの申告**」です。

## 既知の限界（v0.1）

- `show ip bgp` フルテーブルは bgp-summary と誤判定される（安全側に倒れるが情報は出ない）
- NX-OS / EOS / Huawei の固有フォーマットは未検証（generic に落ちる）
- Junos の階層 config・set 形式 config は config パーサ対象外（generic に落ちる）
- トークン数は推定値。モデルごとの実測とはずれる
- `--full` のフルルート出力は普通に巨大（それが --full の意味ではある）

ロードマップは [ROADMAP.md](../ROADMAP.md) を参照。
