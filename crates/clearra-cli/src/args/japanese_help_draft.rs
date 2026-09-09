//! Japanese CLI help prepared for catalog review. Public locale selection remains gated.
use super::{CliHelpTopic, ProductHelpTopic};

pub(super) fn help_body(topic: CliHelpTopic) -> &'static str {
    match topic {
        CliHelpTopic::TopLevel => {
            r#"使い方: clearra [--format text|json|ctk3|fumen] [--lang en|ko] [--verbose] [--verbose-paths] [--include-solution-data] [--solution-output PATH] [--solution-artifact-format compact|json|ctk3|fumen] <pc|pc-scenario|pc-replay|percent|failed-queue|setup-finder|build|build-probability|finesse|damage|spin-finder|ren|spin-structure|build-coverage|rules|scoring|convert|continue|sfinder> [オプション]
グローバルオプションはコマンドの前後のどちらにも指定できます。
finesseの検索: clearra finesse search --base-mask HEX --target-mask HEX --height N (--queue QUEUE | --patterns PATTERN) [--hold empty|PIECE|--no-hold] [--pattern-knowledge both|oracle|visible-7] [--rule RULE]
finesseの評価: clearra finesse score --initial-mask HEX --height N --placements PIECE:rotation:x:y,... (--queue QUEUE | --patterns PATTERN) [--hold empty|PIECE|--no-hold] [--pattern-knowledge both|oracle|visible-7] [--rule RULE]
build-probabilityで最小入力数を調べるには --finesse inputs [--pattern-knowledge both|oracle|visible-7] を追加します。
spin-structureは順序のないミノの集合を検索し、通常スピンとミニスピンの構造を分けて扱います。spin-finder、damage、renの動作は変わりません。
spin-structureの使い方: --pieces IOTSZ [--spin-profile t-spins|t-spins-plus|all-mini|all-mini-plus|all-spin|all-spin-plus] [--lines any|0..4|1+..4+] [--height 4..24] [--fill-bottom N --fill-top N] [--minimality subset-minimal|minimum-piece-count] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads]
--include-solution-dataは、正確なドキュメント出力データをホストに渡すためのJSON専用オプションです。
--solution-outputは、型付きの解法集合ファイルを新規作成し、書き込みを一括で確定します。既定の形式はcompactです。--solution-artifact-formatでcompact、JSON、ネイティブCTK3、ネイティブFumenを選択できます。出力先が既存の場合や、親ディレクトリがシンボリックリンクの場合は拒否されます。
ネイティブCTK3/Fumenのエンコードには、JavaScript、子プロセス、ネットワーク、ブラウザーの実行環境は不要です。
従来のClearraの別名: path=pc-replay、setup=setup-finder、cover=build-coverage
Sfinder-man形式のネイティブ互換コマンドは clearra sfinder <command> にまとめられています。solution-finder 1.43のCLIとの完全互換ではありません。
開幕プリセットの実行例: clearra pc --lines 2"#
        }
        CliHelpTopic::Pc => {
            r#"使い方: clearra pc --lines 2 [--queue IOTSZJL] [--fixed|--observed] [--hold|--no-hold] [--queue-knowledge oracle|visible-7] [--objective all|unique|min-cover|tiling] [--tiling-only] [--solution-probabilities] [--score] [--score-profile tetrio|guideline|jstris-ultra] [--spin-profile t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--initial-b2b N] [--preserve-b2b] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--kick-profile-json JSON] [--backend auto|cpu|gpu|hybrid] [--workers N|--cpu-threads N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--tablebase|--no-tablebase] [--build-dependency-dag|--no-build-dependency-dag] [--no-gpu] [--deterministic] [--max-candidates N] [--max-patterns N] [--max-memory-mib N] [--gpu-device auto|N] [--allow-backend-fallback|--no-backend-fallback]
B2Bを維持できる解法の有無を調べる専用形式: `clearra pc allspin-sol --help`、`clearra pc allspin-pres-chance --help`
--auto-workersはCPUの自動並列数に上限を設定します。小規模な検索を強制的に並列実行するものではありません。
--tiling-onlyは、BuildUpによる構築可能性の検証や確率計算を行わず、正確な幾何学的配置を列挙します。実際には組めない配置が含まれる場合があります。ホールドは使用可能なミノの種類と個数に引き続き影響します。ルール、スコア、B2B、visible-7、tablebase、依存DAG、解法別確率のオプションは利用できません。
ASCとARSはルールレジストリで確認できますが、出現位置からの到達可能性が実装されるまでは検索に使用できません。"#
        }
        CliHelpTopic::PcScenario => {
            r#"使い方: clearra pc-scenario --fixture tests/fixtures/pc/example.json [--verify-expected] [--solution-probabilities] [--backend auto|cpu|gpu|hybrid] [--workers N|--cpu-threads N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--no-gpu]
  または: clearra pc-scenario --field 0x... --queue IOTSZJ --max-pieces 6 [--solution-probabilities] [--rule srs-plus|srs|srs-x|jstris-180|asc|ars|no-kick] [--kick-profile-json JSON] [--workers N|--cpu-threads N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--no-gpu] [--deterministic] [--max-candidates N] [--max-patterns N] [--max-memory-mib N] [--gpu-device auto|N] [--allow-backend-fallback|--no-backend-fallback]
シナリオプリセットをSearchProblemに変換します。解法別確率の出力はCLIの正規化された解法順序を維持します。"#
        }
        CliHelpTopic::Path => {
            r#"使い方: clearra pc-replay --lines 2 [--queue IIOOO] [--fixed|--observed] [--no-hold]
保持された代表リプレイを1件返します。全解法の経路を列挙するSfinderのpathとは異なります。pathは従来の別名として利用できます。"#
        }
        CliHelpTopic::Percent => {
            r#"使い方: clearra percent --queue IOTSZ [--observed|--bag-aligned|--fixed] [--min-len N] [--max-patterns N] [--failed-count N]"#
        }
        CliHelpTopic::FailedQueue => {
            r#"使い方: clearra failed-queue --lines 4 [--patterns P7P3 | --queue IOTSZJL --fixed|--observed] [--hold|--no-hold] [--queue-knowledge oracle|visible-7] [--rule srs-plus|srs|srs-x|jstris-180] [--backend auto|cpu|gpu|hybrid] [--workers N|--cpu-threads N|--auto-workers N] [--tablebase|--no-tablebase] [--build-dependency-dag|--no-build-dependency-dag] [--failed-count N]
指定した逆向き検索のPC目標に到達できないミノ順を、成功集合の正確な補集合として返します。--failed-countを省略すると失敗するミノ順をすべて具体化します。"#
        }
        CliHelpTopic::Setup => {
            r#"使い方: clearra setup-finder --remaining IOTSZJL [--initial-hold empty|I|O|T|S|Z|J|L] [--mode oracle] [--queue-knowledge oracle|visible-7] [--next-cycle-remaining IOTS] [--rule srs-plus|srs|srs-x|jstris-180] [--priority all|build|pc] [--setup-length auto|longer|shorter] [--max-setup-pieces 1..10] [--workers N|--cpu-threads N|--auto-workers N] [--use-all-cpu-threads] [--tablebase|--no-tablebase] [--allow-post-cycle-borrow]
  または: clearra setup-finder --remaining TI --mode qb --qb OS [--queue-knowledge oracle|visible-7] [--next-cycle-remaining OOSITZ] [--initial-hold empty|I|O|T|S|Z|J|L] [--rule srs-plus|srs|srs-x|jstris-180] [--priority all|build|pc] [--setup-length auto|longer|shorter] [--max-setup-pieces 1..10] [--workers N|--cpu-threads N|--auto-workers N] [--use-all-cpu-threads] [--tablebase|--no-tablebase]
PC解法群の途中までの構築を探します。Sfinderのsetupにある必須領域への配置コマンドとは異なります。setupは従来の別名です。--remainingは次のバッグ境界までに残るミノの順序なし集合であり、ホールドを分離する前の集合からPC周期を決定します。2個含められるミノの種類は最大1種類です。重複するミノ1個は自動で初期ホールドに入りますが、PC周期の個数には含まれます。--initial-holdはCLI専用の明示指定で、同じ集合から該当ミノを1個取り除きます。--qbは判明している次のバッグのミノ群を指定し、ミノ順に基づくセットアップ生成を有効にします。これは、全未来を既知とするoracleのカバー率か、先読み7個に基づく厳密な行動方針かを選ぶ--queue-knowledgeとは独立しています。--next-cycle-remainingはPC後に残すホールドとバッグ内の残りミノを厳密に制限し、oracleとQBの両モードで使用できます。--tablebaseは明示的に有効化する機能で、PC4を完成できないと事前計算で確定した状態だけを除外します。それ以外は通常の厳密検索を行います。--ruleはセットアップと完成形のすべてのBuildUp検証に使用するキック表を選びます。既定の検索並列数の上限は、プロセスから利用できる論理プロセッサ数から1を引いた値です。--auto-workersは固定並列実行を強制せず、この自動上限を下げます。--use-all-cpu-threadsを指定すると予約していた1個も使用します。--max-setup-piecesの既定値は9です。完成済みのPC解法を含める場合は10を指定します。優先度allは構築とPCの同時カバー率で順位を付けます。セットアップの長さは独立した設定です。autoはall/buildでは長い形、pcでは短い形を優先します。"#
        }
        CliHelpTopic::Cover => {
            r#"使い方: clearra build-coverage [--template name|--template-json json|--template-file path] [--export-template-json]
Clearraの型付き構築テンプレートを評価します。Sfinderのcoverが受け取る操作列やFumenの入力仕様とは異なります。coverは従来の別名として利用できます。"#
        }
        CliHelpTopic::Rules => {
            r#"使い方: clearra rules <list|inspect|verify|import|export> [--profile id] [--input json]"#
        }
        CliHelpTopic::Scoring => {
            r#"使い方: clearra scoring <list|inspect|import|export> [--profile id] [--input json]"#
        }
        CliHelpTopic::Convert => {
            r#"使い方: clearra convert --from fumen-like --to text|json --input <v115@...>"#
        }
        CliHelpTopic::Continue => r#"使い方: clearra continue <token>"#,
        CliHelpTopic::SpinStructure => {
            r#"使い方: clearra spin-structure search --pieces IOTSZ [共通オプション]
  または: clearra spin-structure cover --pieces IOTSZ [--objective min-cover] [--max-patterns 1..100000] [共通オプション] [--ties --tie-snapshot PATH]
  または: clearra spin-structure guaranteed --pieces IOTSZ [--final-piece T] [--max-patterns 1..100000] [--dependency-report|--no-dependency-report] [共通オプション]
共通オプション: [--board-mask-v1 HEX | --board-mask HEX] [--height 4..24] [--fill-bottom N --fill-top N] [--lines any|0..4|1+..4+] [--spin-profile t-spins|t-spins-plus|all-mini|all-mini-plus|all-spin|all-spin-plus] [--minimality subset-minimal|minimum-piece-count] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads]
すべての経路で、順序のないミノの集合をホールドなしでCPU検索します。バックエンドの代替使用は無効です。ミノ順・パターン、ホールド、GPU、tablebase、明示的なメモリ上限のオプションは利用できません。searchは通常のスピン構造群を返します。coverは正確な最小スピン構造集合を返します。--tiesを省略すると、一定の規則で並べた最初の集合を表示します。--ties --tie-snapshot PATHを明示すると、同じ最小個数を持つすべての最適集合をページ単位で取得できます。guaranteedは最後のミノを固定したうえで、それ以外のミノのすべての異なる順序に対応できる通常の保証付きスピン構造群を返します。通常スピンとミニスピンは別々に扱い、同じミノの文字を複数指定した場合はその個数を維持します。"#
        }
        CliHelpTopic::Sfinder => {
            r#"使い方: clearra sfinder <command> [従来形式の位置引数] [--workers N|--cpu-threads N|--auto-workers N] [--use-all-cpu-threads]
Clearraのネイティブ互換コマンド: path, chance, percent, minimals, score, score-minimals, saves, best-save, cover, setup, congruent, congruent-cover, cover-percent, special-cover, setup-cover, score-finder, pc-setup, best-setup, dpc-finder
Sfinderのspin/spincoverは順序を持たない構造検索です。対応する構造検索とカバー結果の仕様が実装されるまでは明示的にエラーになります。順序を持つClearraの前向き検索spin-finderの別名ではありません。
Sfinder-man形式の一部に対応しており、solution-finder 1.43のCLIとの完全互換ではありません。未対応の従来パラメーターを指定すると明示的にエラーになります。
Clearraのpath/setup/coverは従来のClearraでの意味を維持します。互換コマンドとして使う場合はこの名前空間を使用してください。
*p4、*!、[OISZ]p2などのSfinderのミノ順表記は、この入力境界で正規化されます。
--auto-workersは小規模な検索の並列実行を強制せず、自動並列数を制限します。--workersは固定数のワーカーを明示的に要求します。"#
        }
        CliHelpTopic::Product(topic) => product_help_body(topic),
    }
}

pub(super) fn product_help_body(topic: ProductHelpTopic) -> &'static str {
    match topic {
        ProductHelpTopic::PcTiling => {
            r#"使い方: clearra pc tiling --lines 2 [--patterns PATTERN | --queue QUEUE] [--no-hold] [--backend auto|cpu|gpu|hybrid] [--gpu-device auto|N] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--max-patterns N] [--max-nodes N] [--max-frontier-states N] [--max-candidates N] [--max-memory-mib N] [--allow-backend-fallback|--no-backend-fallback]
  または: clearra pc tiling --board-mask HEX --height 1..6 --pieces N --lines same-as-height [--patterns PATTERN | --queue QUEUE] [--hold empty|PIECE|--no-hold] [バックエンド・リソースオプション]
PC専用の幾何学的配置検索を実行し、供給されるミノと正確に適合する配置群を返します。BuildUp、到達可能性、カバー率、確率、ルール、スピン、B2B、スコア、visible-7、tablebase、依存DAG、実行制約の意味付けは利用できません。実際には組めない配置が含まれる場合があります。汎用形式の`clearra pc --tiling-only`と`clearra pc --objective tiling`は上級者向けの汎用PCリクエストとして維持され、この専用結果と同じ意味にはなりません。"#
        }
        ProductHelpTopic::PcMinimals => {
            r#"使い方: clearra pc minimals --lines 2 [--patterns PATTERN | --queue IOTSZJL] [--hold|--no-hold] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--backend auto|cpu|gpu|hybrid] [--workers N|--auto-workers N] [--max-patterns N] [--max-nodes N] [--max-frontier-states N] [--max-candidates N]
  または: clearra pc minimals --board-mask HEX --height 1..6 --pieces N --lines same-as-height [--patterns PATTERN | --queue QUEUE] [--hold empty|PIECE|--no-hold] [検索オプション]
最小解法集合の専用検索を実行します。入力全体のカバー状況を完全にリプレイ検証した後、その検索条件に対する正確な最小カバーを返します。明示的なメモリ上限、スコア、tiling-only、visible-7、tablebase、依存DAGは利用できません。トップレベルのminimalsとsfinder minimalsは従来互換の汎用結果を返します。"#
        }
        ProductHelpTopic::PcPath => {
            r#"使い方: clearra pc path --lines 2|4|6 (--queue QUEUE | --patterns PATTERN) [--hold|--no-hold] [--rule RULE] [検索オプション]
  または: clearra pc path --board-mask HEX --height 1..6 --pieces N --lines same-as-height (--queue QUEUE | --patterns PATTERN) [--hold empty|PIECE|--no-hold] [--rule RULE] [検索オプション]
objective all、count allで完全なリプレイ経路を専用検索します。各手順は配置、元のミノ列、ホールドと読み取り位置の遷移、消費ミノ数、ライン消去を保持します。最適集合の同率候補ではなく、同率集合のメタデータやページ継続カーソルは含みません。"#
        }
        ProductHelpTopic::PcChance => {
            r#"使い方: clearra pc chance --lines 2 [--patterns PATTERN | --queue IOTSZJL] [--hold|--no-hold] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--backend auto|cpu|gpu|hybrid] [--workers N|--auto-workers N] [--max-patterns N]
PC確率の専用検索を実行し、入力ミノ順全体に対する完全な確率を返します。トップレベルのchanceとpercentは従来互換の汎用結果を返します。"#
        }
        ProductHelpTopic::PcScore => {
            r#"使い方: clearra pc score --lines 2 [--patterns PATTERN | --queue IOTSZJL] [--hold|--no-hold] [--score-profile tetrio|guideline|jstris-ultra] [--spin-profile disabled|t-spin-simple|t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--initial-b2b N] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup]
  または: clearra pc score --board-mask HEX --height 1..6 --pieces N --lines same-as-height [--patterns PATTERN | --queue QUEUE] [スコアオプション] [CPUワーカーオプション]
ネイティブCPU実行には通常のローカルワーカー設定を使用します。自動実行は--use-all-cpu-threadsがない限り論理プロセッサを1個予約し、--workersは固定並列数を指定します。ブラウザーでは管理側がNを保持し、分離された各WASM子処理を1ワーカーに正規化してワーカープールの入れ子を防ぎます。入力は元のミノ16個以内と、因数分解されたパターン式1個に制限されます。P7P7P2は記号的に処理できます。PC盤面ごとの平均スコアを返します。スコアは基本的な近似値であり、プロファイルごとの厳密値ではありません。トップレベルのscoreとsfinder scoreは従来互換の汎用結果を返します。"#
        }
        ProductHelpTopic::PcScoreFinder => {
            r#"使い方: clearra pc score-finder --lines 2|4|6 --queue QUEUE [--hold|--no-hold] [--initial-b2b 0|1] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--ties]
  または: clearra pc score-finder --board-mask HEX --height 1..6 --pieces N --lines same-as-height --queue QUEUE [--hold empty|PIECE|--no-hold] [--initial-b2b 0|1] [--rule RULE] [CPUワーカーオプション] [--ties]
専用のjstris-ultraスコアプロファイルとt-spinsスピンプロファイルを使用し、固定ミノ順の最高スコアを検索します。ネイティブCPU実行は通常のローカルワーカー設定を使用します。自動実行は--use-all-cpu-threadsがない限り論理プロセッサを1個予約し、--workersは固定並列数を指定します。ブラウザーでは管理側がNを保持し、分離された各WASM子処理は1ワーカーを使用します。最高スコアの同率判定と順序には整数のスコアだけを使用します。攻撃力は参考値であり、同率の順位決定には使用しません。通常の結果に最適集合の同率メタデータはありません。--tiesを明示すると同じ最高スコアを持つすべての手順を通常の解法群として表示します。--tie-snapshotは使用できません。"#
        }
        ProductHelpTopic::PcScoreMinimals => {
            r#"使い方: clearra pc score-minimals --lines 2 [--patterns PATTERN | --queue IOTSZJL] [--hold|--no-hold] [--score-profile tetrio|guideline|jstris-ultra] [--spin-profile disabled|t-spin-simple|t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--initial-b2b N] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--ties --tie-snapshot PATH]
  または: clearra pc score-minimals --board-mask HEX --height 1..6 --pieces N --lines same-as-height [--patterns PATTERN | --queue QUEUE] [スコアオプション] [CPUワーカーオプション] [--ties --tie-snapshot PATH]
スコアだけを基準とするB-optionの最高スコア最小集合検索を実行します。ネイティブCPU実行は通常のローカルワーカー設定を使用します。自動実行は--use-all-cpu-threadsがない限り論理プロセッサを1個予約し、--workersは固定並列数を指定します。ブラウザーでは管理側がNを保持し、分離された各WASM子処理は1ワーカーを使用します。スコアの同率判定、候補の適格性、並び順、集合の構成、決定的な選択に攻撃力は使わず、参考値としてのみ扱います。--tiesがなければ一定の規則で並べた最初の集合を表示します。--tiesを明示すると、再開可能な正確なスナップショットを作成します。同じ最小個数を持つすべての最適集合を`clearra continue --tie-snapshot PATH --tie-cursor TOKEN`でページ単位で取得できます。"#
        }
        ProductHelpTopic::PcSaves => {
            r#"使い方: clearra pc saves --lines 2|4|6 [--patterns PATTERN] [--hold|--no-hold] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [検索オプション]
  または: clearra pc saves --board-mask HEX --height 1..6 --pieces N --lines same-as-height [--patterns PATTERN] [--hold empty|PIECE|--no-hold]
残しミノのグループを返します。各グループは終了時のホールドと現在のバッグに残るミノの多重集合で構成され、元の各パターン内で重複を除きます。入力全体に対する無条件確率と、PCに成功するミノ順に限った条件付き確率を含みます。固定ミノ順、観測済みまたはvisible-sevenの入力、明示的なメモリ上限、スコア、幾何学的配置、解法別確率は、必要な固定バッグ境界の根拠を提供できないため拒否されます。"#
        }
        ProductHelpTopic::PcBestSave => {
            r#"使い方: clearra pc best-save --lines 2|4|6 [--patterns PATTERN] [--hold|--no-hold] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [検索オプション]
  または: clearra pc best-save --board-mask HEX --height 1..6 --pieces N --lines same-as-height [--patterns PATTERN] [--hold empty|PIECE|--no-hold]
規定の残しミノの重みに基づいて最良のグループを返します。重みの合計（T6/I4/O3/J1/L1/S0/Z0）、min(J,L)、入力全体に対するグループの正確な無条件確率の順で比較します。完全に同率の最良グループはすべて通常のリスト項目として返し、最適集合の同率処理は使用しません。入力の根拠に関する制限はPC savesと同じです。固定バッグ境界の出所が必要であり、固定ミノ順や観測済み・visible-sevenの入力は拒否されます。"#
        }
        ProductHelpTopic::PcFailedQueue => {
            r#"使い方: clearra pc failed-queue --lines 4 [--patterns P7P3 | --queue IOTSZJL] [--failed-count N]
失敗するミノ順の専用検索を実行します。トップレベルのfailed-queueとfailed_queueは従来互換の汎用Percentリクエストとして維持されます。"#
        }
        ProductHelpTopic::PcAllSpinSolution => {
            r#"使い方: clearra pc allspin-sol --lines 2|4|6 --queue QUEUE --spin-profile t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus [--no-hold] [検索オプション]
  または: clearra pc allspin-sol --board-mask HEX --height 1..6 --pieces N [--lines same-as-height] --queue QUEUE --spin-profile PROFILE [--no-hold] [検索オプション]
検索オプション: [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--backend auto|cpu|gpu|hybrid] [--gpu-device auto|N] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--tablebase|--no-tablebase] [--build-dependency-dag|--no-build-dependency-dag] [--max-patterns N] [--max-nodes N] [--max-frontier-states N] [--max-candidates N] [--max-memory-mib N] [--allow-backend-fallback|--no-backend-fallback]
元の固定ミノ順を正確に1件指定して、固定とライン消去を逆にたどるPC検索を行います。B2Bを維持できる手順が存在する場合は、一定の規則で選んだ手順を返します。分母は具体化された元のミノ順1件であり、ホールドや経路の重複は数えません。任意のboard-mask/height/piecesの組は初期盤面を表します。盤面を空にする目標は暗黙に固定され、目標盤面の入力はありません。この組と--linesを併用する場合は--heightと同じ値が必要です。この形式はoracle-fixed専用であり、FILEやローカルパス、visible-7、スコアや目的の選択、シナリオのホールド枠の上書き、呼び出し側による--preserve-b2b指定は拒否されます。Clearraは明示的に選択された6種類のスピン・リプレイ仕様を使用します。sfinderbotのallspin_sol_finderとはコマンドの目的のみ互換で、従来の認識規則との完全一致を保証しません。"#
        }
        ProductHelpTopic::PcAllSpinPreservationChance => {
            r#"使い方: clearra pc allspin-pres-chance --lines 2|4|6 --patterns PATTERN --spin-profile t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus [--no-hold] [検索オプション]
  または: clearra pc allspin-pres-chance --board-mask HEX --height 1..6 --pieces N [--lines same-as-height] --patterns PATTERN --spin-profile PROFILE [--no-hold] [検索オプション]
検索オプション: [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--backend auto|cpu|gpu|hybrid] [--gpu-device auto|N] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--gpu-warmup] [--tablebase|--no-tablebase] [--build-dependency-dag|--no-build-dependency-dag] [--max-patterns N] [--max-nodes N] [--max-frontier-states N] [--max-candidates N] [--max-memory-mib N] [--allow-backend-fallback|--no-backend-fallback]
具体化された元のミノ順パターンに対し、固定とライン消去を逆にたどるPC検索を行います。B2Bを維持する手順が存在するミノ順の件数、元のミノ順の総数、入力の確率、計算の完全性を返します。各ミノ順は1回だけ数え、ホールドや経路の重複は数えません。任意のboard-mask/height/piecesの組は初期盤面を表します。盤面を空にする目標は暗黙に固定され、目標盤面の入力はありません。この組と--linesを併用する場合は--heightと同じ値が必要です。この形式はoracle-fixed専用であり、FILEやローカルパス、visible-7、スコアや目的の選択、シナリオのホールド枠の上書き、呼び出し側による--preserve-b2b指定は拒否されます。Clearraは明示的に選択された6種類のスピン・リプレイ仕様を使用します。sfinderbotのallspin_pres_chanceとはコマンドの目的のみ互換で、従来の認識規則との完全一致を保証しません。"#
        }
        ProductHelpTopic::BuildV2 => {
            r#"使い方: clearra build cover --base-mask HEX --target-mask HEX --height N (--queue QUEUE | --patterns PATTERN) [--source-pieces N] [--objective min-cover|max-probability-minimum] [Build実行オプション]
  または: clearra build <setup|congruent|congruent-cover|setup-cover|setup-cover-percent|setup-cover-score> --target-format ctk3|fumen --target-document DOCUMENT (--queue QUEUE | --patterns PATTERN) [型付きBuildオプション]
  または: clearra build evaluate <cover|minimals|score|b2b-cover|cover-percent> --solution-format ctk3|fumen --solution-document DOCUMENT (--queue QUEUE | --patterns PATTERN) [型付きBuildオプション]
目標ドキュメントと入力済み解法ドキュメントは異なる型として扱い、相互に代用できません。すべての形式で--queueと--patternsのいずれか一方が必須です。--queue-knowledge oracle|visible-7、--hold PIECE|--no-hold、--rule RULE、--max-patterns N、--max-nodes N、--max-frontier-states N、--max-candidates N、--workers N|--auto-workers N|--use-all-cpu-threads、--cpu-warmupを使用できます。--objective all|unique|min-cover|max-probability-minimum|max-score-coverのうち、各形式で許可された目的だけを指定できます。互換の別名はminimum-coverのみです。スコア形式だけが--score-profile tetrio|guideline|jstris-ultraと--initial-b2b 0..65535を受け付けます。正確な最適集合を返す形式（cover、congruent-cover、setup-cover、setup-cover-score、evaluate minimals、evaluate score）で別の同率解を取得するには、--ties --tie-snapshot PATHの明示が必要です。通常の解法群や確率結果では使用しません。スコアの同率判定と順序に攻撃力は使用しません。型付きBuildはv0.8ではCPU専用です。有限な応答の保証が実装されるまで--max-memory-mibは拒否されます。"#
        }
        ProductHelpTopic::BuildProbability => {
            r#"使い方: clearra build-probability --base-mask HEX --target-mask HEX --height 1..24 (--queue QUEUE | --patterns PATTERN) [--hold empty|PIECE|--no-hold] [--source-pieces N] [--aggregate buildability|tiling|spin] [--result-mode all-solutions|complete-replay-paths|field-average-score|fixed-queue-maximum-score|highest-score-minimum-set|failed-queues] [--paths|--score] [--score-profile tetrio|guideline|jstris-ultra] [--initial-b2b N] [--failed-count N] [--tiling-only] [--solution-probabilities] [--spin-profile t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--preserve-b2b] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--build-dependency-dag|--no-build-dependency-dag] [--finesse off|inputs] [--pattern-knowledge both|oracle|visible-7] [--include-mirror|--no-mirror] [--backend auto|cpu|gpu|hybrid] [--workers N|--auto-workers N] [--use-all-cpu-threads] [--cpu-warmup] [--max-patterns N] [--max-candidates N] [--max-memory-mib N] [--allow-backend-fallback|--no-backend-fallback]
エンジン側の集約と結果の集約は別の概念であり、組み合わせの可否が明示されています。現在、all以外の結果モードには構築可能性の集約が必要で、互換性のないtiling/spinとの組み合わせは拒否されます。完全リプレイ経路は操作、固定、ライン消去を網羅する手順です。リプレイとスコアの結果モードでは、既存セルと目標セルが下6行以内に収まる必要があります。その上の空の表示行は使用できます。より高い位置にセルがある構築にはall-solutions、failed-queues、build coverの最小集合を使用できます。盤面平均スコアは成功したすべての正規化盤面と、失敗パターンを0点として含めた入力全体のスコアを返します。固定ミノ順の最高スコアには正確なミノ順1件が必要で、スコアが同率の候補をすべて保持します。最高スコア最小集合は、成功する各パターンの最高スコアに並ぶすべての候補から正確な最小集合を求めます。攻撃力は参考値としてのみ扱います。失敗するミノ順は構築成功集合の正確な補集合です。--failed-countは表示例の数だけを制限します。最小構築集合には`clearra build cover --objective min-cover`を使用します。主指標は引き続き全未来を既知とするoracleの構築確率です。--solution-probabilitiesは正確な解法別確率を追加します。--spin-profileには--aggregate spinまたは--preserve-b2bが必要です。--pattern-knowledgeには--finesse inputsが必要です。tiling集約ではルール、スピン、B2B、依存DAG、解法別確率、finesseのオプションは拒否されます。"#
        }
        ProductHelpTopic::Finesse => {
            r#"使い方: clearra finesse search --base-mask HEX --target-mask HEX --height N (--queue QUEUE | --patterns PATTERN) [--hold empty|PIECE|--no-hold] [--pattern-knowledge both|oracle|visible-7] [--rule RULE] [--workers N|--auto-workers N]
  または: clearra finesse score --initial-mask HEX --height N --placements PIECE:rotation:x:y,... (--queue QUEUE | --patterns PATTERN) [--hold empty|PIECE|--no-hold] [--pattern-knowledge both|oracle|visible-7] [--rule RULE]"#
        }
        ProductHelpTopic::Damage => {
            r#"使い方: clearra damage --board-mask HEX --height 1..24 --queue QUEUE [--hold|--no-hold] [--spin-profile disabled|t-spin-simple|t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--initial-combo 0..65535] [--initial-b2b 0..65535] [--preserve-b2b] [--minimum-damage 0..4294967295] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads]
既定のスピンプロファイルはall-mini-plusです。--minimum-damageを指定すると指定値以上を検索し、省略すると最大攻撃力の結果を返します。"#
        }
        ProductHelpTopic::SpinFinder => {
            r#"使い方: clearra spin-finder --board-mask HEX --height 1..24 (--queue QUEUE | --patterns PATTERN) [--hold|--no-hold] [--spin-profile t-spin-simple|t-spins|t-spins-plus|all-spin|all-spin-plus|all-mini|all-mini-plus] [--lines any|0..4|1+..4+] [--spin-category any|t|other] [--initial-combo 0..65535] [--initial-b2b 0..65535] [--preserve-b2b] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads]
既定のスピンプロファイルはt-spinsです。--spin-category otherにはall-spinまたはall-miniのプロファイルが必要です。"#
        }
        ProductHelpTopic::Ren => {
            r#"使い方: clearra ren --board-mask HEX --height 1..24 --queue QUEUE [--hold|--no-hold] [--rule srs-plus|srs|srs-x|jstris-180|no-kick] [--workers N|--auto-workers N] [--use-all-cpu-threads]
最大22個の固定ミノ順について、厳密に最大のRENを持つ正規化された手順をすべて探します。初期盤面で埋まっている行は正規化時に消去し、RENには数えません。採用する各固定で1行以上を消去する必要があり、最初の固定でラインを消去できなければ解法はありません。ホールドは空で開始し、既定で有効です。スピンと攻撃力のスコア設定は利用できません。"#
        }
        ProductHelpTopic::MappedCompatibility => {
            r#"使い方: clearra <mapped-command> [従来互換オプション]
このコマンドは選定された互換マッピングです。対応するコマンド一覧は`clearra sfinder --help`を参照してください。対象外のパラメーターは明示的にエラーになります。"#
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_i18n::LanguageId;

    #[test]
    fn every_help_topic_has_japanese_body_without_enabling_the_locale() {
        let topics = [
            CliHelpTopic::TopLevel,
            CliHelpTopic::Pc,
            CliHelpTopic::PcScenario,
            CliHelpTopic::Path,
            CliHelpTopic::Percent,
            CliHelpTopic::FailedQueue,
            CliHelpTopic::Setup,
            CliHelpTopic::Cover,
            CliHelpTopic::Rules,
            CliHelpTopic::Scoring,
            CliHelpTopic::Convert,
            CliHelpTopic::Continue,
            CliHelpTopic::SpinStructure,
            CliHelpTopic::Sfinder,
            CliHelpTopic::Product(ProductHelpTopic::PcTiling),
            CliHelpTopic::Product(ProductHelpTopic::PcMinimals),
            CliHelpTopic::Product(ProductHelpTopic::PcPath),
            CliHelpTopic::Product(ProductHelpTopic::PcChance),
            CliHelpTopic::Product(ProductHelpTopic::PcScore),
            CliHelpTopic::Product(ProductHelpTopic::PcScoreFinder),
            CliHelpTopic::Product(ProductHelpTopic::PcScoreMinimals),
            CliHelpTopic::Product(ProductHelpTopic::PcSaves),
            CliHelpTopic::Product(ProductHelpTopic::PcBestSave),
            CliHelpTopic::Product(ProductHelpTopic::PcFailedQueue),
            CliHelpTopic::Product(ProductHelpTopic::PcAllSpinSolution),
            CliHelpTopic::Product(ProductHelpTopic::PcAllSpinPreservationChance),
            CliHelpTopic::Product(ProductHelpTopic::BuildV2),
            CliHelpTopic::Product(ProductHelpTopic::BuildProbability),
            CliHelpTopic::Product(ProductHelpTopic::Finesse),
            CliHelpTopic::Product(ProductHelpTopic::Damage),
            CliHelpTopic::Product(ProductHelpTopic::SpinFinder),
            CliHelpTopic::Product(ProductHelpTopic::Ren),
            CliHelpTopic::Product(ProductHelpTopic::MappedCompatibility),
        ];
        for topic in topics {
            let body = help_body(topic);
            assert!(body.starts_with("使い方: clearra "), "{topic:?}");
            assert!(!body.contains("usage:"), "{topic:?}");
            let rendered = topic.into_output(LanguageId::Ja);
            assert!(rendered.stdout().contains(body), "{topic:?}");
        }
        assert_eq!(LanguageId::parse("ja"), None);
        assert_eq!(LanguageId::parse_known("ja-JP"), Some(LanguageId::Ja));
        assert!(!LanguageId::Ja.is_released());
    }
}
