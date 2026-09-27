import type { WorkspaceLanguage } from './workspaceI18n';
const text = {
  start: ['Start', '시작', '開始'],
  middle: ['Middle', '중간', '中間'],
  result: ['Result', '결과', '結果'],
  first: ['Start → Middle', '시작 → 중간', '開始 → 中間'],
  second: ['Middle → Result', '중간 → 결과', '中間 → 結果'],
  firstHelp: ['The first supply. Some pieces may build the result early; the following supply can complete what remains in the middle.', '먼저 나오는 공급입니다. 일부 미노로 결과를 먼저 구축하고, 뒤의 공급으로 중간의 빈 배치를 완성할 수 있습니다.', '先に出るミノです。結果を先行して作り、残った中間の配置を後の供給で完成できます。'],
  secondHelp: ['Continues after the first supply, without resetting hold. Fixed queues and patterns use the same grammar.', '앞의 공급에 이어서 나옵니다. 홀드는 초기화되지 않으며, 고정 큐와 패턴은 같은 문법을 사용합니다.', '前の供給に続き、ホールドはリセットしません。固定キューとパターンは同じ文法です。'],
  fieldsHelp: ['Start is existing material. Middle is what to add to it. Result is what to add after the completed middle rows clear. Gray references do not change the selected field.', '시작은 기존 블록, 중간은 그 위에 추가할 영역입니다. 결과는 중간의 완성 줄이 지워진 뒤 추가할 영역입니다. 참고 색상은 선택한 필드의 입력을 바꾸지 않습니다.', '開始は既存ブロック、中間は追加領域です。結果は中間の完成行が消えた後の追加領域です。参照色は入力を変更しません。'],
  context: ['Show field context', '다른 필드 함께 보기', '他のフィールドを表示'],
  early: ['Maximum early placements', '최대 선행 배치', '先行配置の最大数'],
  auto: ['Auto — all feasible counts', '자동 — 가능한 모든 개수', '自動 — 可能な全個数'],
  earlyHelp: ['Limits placements used early, not search work. Auto covers every feasible count in the supplied problem.', '선행 사용하는 배치 수를 정합니다. 탐색 자원 한도가 아닙니다. 자동은 입력한 문제에서 가능한 모든 개수를 허용합니다.', '先行配置数の指定であり探索量の制限ではありません。自動は入力内の可能な全個数を許可します。'],
  exchange: ['Allow different-piece repayment', '다른 종류의 미노로 반환 허용', '異なる種類のミノで補完'],
  exchangeHelp: ['Off: preserve each supply’s piece types in its target. On: redistribute types between middle and result while conserving the actual combined supply. Pieces never change shape.', '끄면 각 공급의 미노 종류별 구성을 유지합니다. 켜면 두 공급을 합친 종류별 수는 유지하면서 중간과 결과에 쓰는 구성을 바꿀 수 있습니다. 미노의 모양을 바꾸지는 않습니다.', 'オフは各供給の種類構成を保持します。オンは全供給の個数を保持したまま中間と結果の構成を変えます。ミノ自体の形は変えません。'],
  b2bHelp: ['Checks every actual clear, including middle placements completed later. Isolated middle candidates are not rejected for B2B alone.', '나중에 완성하는 중간 배치를 포함해 실제 줄 삭제를 검사합니다. 중간을 단독으로 만들 때의 B2B 실패만으로 후보를 버리지 않습니다.', '後から完成する中間配置も含め実際の消去を検証します。中間単独のB2B失敗だけでは候補を除外しません。'],
  invalid: ['Check the two supplies and the non-overlapping four-cell target regions. Pattern syntax is verified by the shared engine parser when running.', '두 공급과 겹치지 않는 4칸 단위의 목표 영역을 확인하세요. 패턴 문법은 실행 시 공통 엔진 파서가 검사합니다.', '二つの供給と重ならない4セル単位の領域を確認してください。文法は実行時に共通パーサーが検証します。'],
  normal: ['Normal connection', '정상 연결', '通常接続'],
  recovery: ['Additional recovery', '추가 리커버리', '追加リカバリー'],
  unavailable: ['No connection', '연결 불가', '接続なし'],
  examples: ['Representative paths, not an exhaustive solution list', '대표 경로 — 전체 해법 목록이 아닙니다', '代表経路 — 全解一覧ではありません'],
  examplesHelp: ['Probabilities count supply pairs once, regardless of how many paths solve them. Copy exports all shown representative paths as ordered lock snapshots.', '확률은 같은 공급 조합을 경로 수와 관계없이 한 번만 셉니다. 복사는 표시된 대표 경로 모두를 실제 배치 순서의 필드 페이지로 내보냅니다.', '確率は同じ供給組を経路数によらず一度だけ数えます。コピーは表示中の全代表経路を配置順のフィールドページとして出力します。'],
  details: ['Placement details', '배치 상세', '配置詳細'],
  used: ['Used pieces', '사용한 미노', '使用ミノ'],
  exchanged: ['Piece balance', '종류별 반환 차이', '種類別差分'],
  noPath: ['No legal connection exists within these complete supply inputs and targets.', '이 공급과 목표 영역의 완전 탐색에서 연결을 찾지 못했습니다.', 'この供給と目標領域の完全探索で接続が見つかりませんでした。'],
  notReady: ['Enter the fields and two supplies, then run the search.', '필드와 두 공급을 입력하고 탐색하세요.', 'フィールドと二つの供給を入力して探索してください。'],
  invalidResult: ['The returned recovery evidence is inconsistent. It is not displayed as a valid solution.', '반환된 리커버리 증거가 일치하지 않습니다. 올바른 해법으로 표시하지 않습니다.', '返された証拠に矛盾があります。有効な解として表示しません。'],
  evaluated: ['Evaluated supply pairs', '검사한 공급 조합', '検証した供給組'],
} as const;
export type RecoveryBuildMessage = keyof typeof text;
export function recoveryBuildMessage(language: WorkspaceLanguage, key: RecoveryBuildMessage): string {
  return text[key][language === 'ko' ? 1 : language === 'ja' ? 2 : 0];
}
