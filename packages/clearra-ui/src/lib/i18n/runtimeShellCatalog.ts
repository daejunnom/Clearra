import type { WorkspaceLocale } from './languageManifest';
import { formatWasmTerminalLine, type WasmTerminalLine } from '../wasm/wasmTerminalTranscript';

// Japanese is available to translation tooling here; shell components resolve
// their runtime locale through the released-language manifest before rendering.
export const RUNTIME_SHELL_MESSAGES = {
  run: { en: 'Run', ko: '실행', ja: '実行' },
  cancel: { en: 'Cancel', ko: '취소', ja: 'キャンセル' },
  canonicalRequest: { en: 'Canonical CLI request', ko: '표준 CLI 요청', ja: '正規CLIリクエスト' },
  cliRequest: { en: 'CLI request', ko: 'CLI 요청', ja: 'CLIリクエスト' },
  language: { en: 'Language', ko: '언어', ja: '言語' },
  arguments: { en: 'Arguments', ko: '인수', ja: '引数' },
  backendStatus: { en: 'Backend status', ko: '백엔드 상태', ja: 'バックエンドの状態' },
  backend: { en: 'Backend', ko: '백엔드', ja: 'バックエンド' },
  requested: { en: 'Requested', ko: '요청됨', ja: '要求した設定' },
  selected: { en: 'Selected', ko: '선택됨', ja: '選択された設定' },
  fallback: { en: 'Fallback', ko: '대체 처리', ja: '代替処理' },
  boundary: { en: 'Boundary', ko: '실행 경계', ja: '実行境界' },
  job: { en: 'Job', ko: '작업', ja: 'ジョブ' },
  jobProgress: { en: 'Job progress', ko: '작업 진행 상황', ja: 'ジョブの進捗' },
  budget: { en: 'Budget', ko: '리소스 한도', ja: 'リソース上限' },
  memory: { en: 'Memory', ko: '메모리', ja: 'メモリ' },
  complete: { en: 'Complete', ko: '완료 여부', ja: '計算の完全性' },
  diagnostics: { en: 'Diagnostics', ko: '진단', ja: '診断' },
  noDiagnostics: { en: 'None', ko: '없음', ja: 'なし' },
  result: { en: 'Result', ko: '결과', ja: '結果' },
  renderStatus: { en: 'Render status', ko: '렌더링 상태', ja: '描画の状態' },
  render: { en: 'Render', ko: '렌더링', ja: '描画' },
  exact: { en: 'Exact', ko: '정확한 렌더링', ja: '厳密な描画' },
  reason: { en: 'Reason', ko: '이유', ja: '理由' },
  command: { en: 'Command', ko: '명령', ja: 'コマンド' },
  runtime: { en: 'Runtime', ko: '실행 환경', ja: '実行環境' },
  status: { en: 'Status', ko: '상태', ja: '状態' },
  appStatus: { en: 'App Status', ko: '앱 상태', ja: 'アプリの状態' },
  worker: { en: 'Worker', ko: '워커', ja: 'ワーカー' },
  progress: { en: 'Progress', ko: '진행 상황', ja: '進捗' },
  output: { en: 'Output', ko: '출력', ja: '出力' },
  workers: { en: 'Workers', ko: '워커 수', ja: 'ワーカー数' },
  solutions: { en: 'Solutions', ko: '해법 수', ja: '解法数' },
  solutionHash: { en: 'Solution hash', ko: '해법 해시', ja: '解法のハッシュ' },
  coverage: { en: 'Coverage', ko: '커버율', ja: 'カバー率' },
  connected: { en: 'Connected', ko: '연결 여부', ja: '接続状態' },
  trust: { en: 'Trust', ko: '신뢰 상태', ja: '信頼状態' },
  shader: { en: 'Shader', ko: '셰이더', ja: 'シェーダー' },
  warmup: { en: 'Warmup', ko: '사전 준비', ja: '事前準備' },
  sessionReused: { en: 'Session reused', ko: '세션 재사용', ja: 'セッションの再利用' },
  terminalOutput: { en: 'terminal-like output', ko: '터미널 출력', ja: 'ターミナル出力' },
  pending: { en: 'pending', ko: '대기 중', ja: '待機中' },
  none: { en: 'none', ko: '없음', ja: 'なし' },
  used: { en: 'used', ko: '사용됨', ja: '使用済み' },
  unknown: { en: 'unknown', ko: '알 수 없음', ja: '不明' },
  supported: { en: 'supported', ko: '지원됨', ja: '対応' },
  unsupported: { en: 'unsupported', ko: '지원되지 않음', ja: '未対応' },
  parallel: { en: 'parallel', ko: '병렬', ja: '並列' },
  serial: { en: 'serial', ko: '직렬', ja: '逐次' },
  notCalculated: { en: 'not calculated', ko: '계산하지 않음', ja: '未計算' },
  true: { en: 'true', ko: '예', ja: 'はい' },
  false: { en: 'false', ko: '아니요', ja: 'いいえ' },
  idle: { en: 'idle', ko: '대기', ja: '待機' },
  validating: { en: 'validating', ko: '검증 중', ja: '検証中' },
  running: { en: 'running', ko: '실행 중', ja: '実行中' },
  cancelling: { en: 'cancelling', ko: '취소 중', ja: 'キャンセル中' },
  completed: { en: 'completed', ko: '완료', ja: '完了' },
  cancelled: { en: 'cancelled', ko: '취소됨', ja: 'キャンセル済み' },
  terminated: { en: 'terminated', ko: '강제 종료됨', ja: '強制終了済み' },
  failed: { en: 'failed', ko: '실패', ja: '失敗' },
  success: { en: 'success', ko: '성공', ja: '成功' },
  validationFailed: { en: 'validation-failed', ko: '검증 실패', ja: '検証失敗' },
  executionFailed: { en: 'execution-failed', ko: '실행 실패', ja: '実行失敗' },
  error: { en: 'error', ko: '오류', ja: 'エラー' },
  warning: { en: 'warning', ko: '경고', ja: '警告' },
  info: { en: 'info', ko: '정보', ja: '情報' },
  auto: { en: 'auto', ko: '자동', ja: '自動' },
  hybrid: { en: 'hybrid', ko: '하이브리드', ja: 'ハイブリッド' },
  available: { en: 'available', ko: '사용 가능', ja: '利用可能' },
  unavailable: { en: 'unavailable', ko: '사용 불가', ja: '利用不可' },
  clean: { en: 'clean', ko: '문제없음', ja: '問題なし' },
  notClean: { en: 'not-clean', ko: '문제 있음', ja: '問題あり' },
  loading: { en: 'loading', ko: '불러오는 중', ja: '読み込み中' },
  ready: { en: 'ready', ko: '준비됨', ja: '準備完了' },
  disabled: { en: 'disabled', ko: '비활성화', ja: '無効' },
  partial: { en: 'partial', ko: '부분 결과', ja: '一部のみ' },
  incomplete: { en: 'incomplete', ko: '불완전', ja: '不完全' },
  completeValue: { en: 'complete', ko: '완료', ja: '完了' },
  notExecuted: { en: 'not-executed', ko: '실행하지 않음', ja: '未実行' },
  workersValue: { en: '{count} ({mode})', ko: '{count} ({mode})', ja: '{count}（{mode}）' },
  // Prepared for a future typed command-display entry. Existing "$ ..."
  // transcript strings stay verbatim so user-supplied commands are never edited.
  commandTruncated: { en: '{command}... ({count} characters)', ko: '{command}... ({count}자)', ja: '{command}...（{count}文字）' },
  runtimeReady: { en: 'clearra web runtime ready', ko: 'Clearra 웹 실행 환경이 준비되었습니다.', ja: 'ClearraのWeb実行環境の準備ができました。' },
  jobStarted: { en: 'job {jobId} started', ko: '작업 {jobId} 시작됨', ja: 'ジョブ{jobId}を開始しました。' },
  jobCancelled: { en: 'job cancelled', ko: '작업이 취소되었습니다.', ja: 'ジョブをキャンセルしました。' },
  jobCancelledReleased: { en: 'job cancelled; computation scope released', ko: '작업이 취소되었으며 계산 스코프가 해제되었습니다.', ja: 'ジョブをキャンセルし、計算スコープを解放しました。' },
  executionTerminated: { en: 'WASM execution was force-terminated', ko: 'WASM 실행이 강제 종료되었습니다.', ja: 'WASMの実行を強制終了しました。' },
  executionFailure: { en: 'WASM execution failed', ko: 'WASM 실행에 실패했습니다.', ja: 'WASMの実行に失敗しました。' },
  resourceMismatch: { en: 'WASM failed-event resource evidence was inconsistent', ko: 'WASM 실패 이벤트의 리소스 근거가 일치하지 않습니다.', ja: 'WASMの失敗イベントのリソース根拠が一致しません。' },
  partialResult: { en: 'partial: {label}', ko: '부분 결과: {label}', ja: '部分結果: {label}' },
  workerRequired: { en: 'A browser worker factory is required to start the WASM runtime.', ko: 'WASM 실행 환경을 시작하려면 브라우저 워커 생성기가 필요합니다.', ja: 'WASM実行環境を開始するにはブラウザーワーカーの生成器が必要です。' },
  replacedSolutionPages: { en: 'a new search replaced the previous solution pages', ko: '새 검색이 이전 해법 페이지를 대체했습니다.', ja: '新しい検索で以前の解法ページを置き換えました。' },
  replacedProductPages: { en: 'a new search replaced the previous product pages', ko: '새 검색이 이전 결과 페이지를 대체했습니다.', ja: '新しい検索で以前の結果ページを置き換えました。' },
  productPageCancelled: { en: 'product page runtime was cancelled', ko: '결과 페이지 처리가 취소되었습니다.', ja: '結果ページの処理をキャンセルしました。' },
  solutionPageUnavailable: { en: 'solution page runtime is not available', ko: '해법 페이지 실행 환경을 사용할 수 없습니다.', ja: '解法ページの実行環境を利用できません。' },
  productPageReleased: { en: 'product page runtime was released', ko: '결과 페이지 실행 환경이 해제되었습니다.', ja: '結果ページの実行環境を解放しました。' },
  productPageUnavailable: { en: 'product page runtime is not available', ko: '결과 페이지 실행 환경을 사용할 수 없습니다.', ja: '結果ページの実行環境を利用できません。' },
  ownerDisposed: { en: 'The WASM runtime owner was disposed while a search was active; the worker tree was force-terminated.', ko: '검색 중 WASM 실행 환경의 소유자가 해제되어 워커 트리가 강제 종료되었습니다.', ja: '検索中にWASM実行環境の所有者が破棄されたため、ワーカーツリーを強制終了しました。' },
  workerCrashed: { en: 'WASM worker crashed', ko: 'WASM 워커가 비정상 종료되었습니다.', ja: 'WASMワーカーが異常終了しました。' },
  invalidWorkerMessage: { en: 'WASM worker returned an invalid message', ko: 'WASM 워커가 잘못된 메시지를 반환했습니다.', ja: 'WASMワーカーが無効なメッセージを返しました。' },
  cancellationDeadline: { en: 'The search did not acknowledge cooperative cancellation before the deadline; the worker tree was force-terminated.', ko: '검색이 제한 시간 안에 협력적 취소를 확인하지 않아 워커 트리가 강제 종료되었습니다.', ja: '制限時間内に検索が協調キャンセルに応答しなかったため、ワーカーツリーを強制終了しました。' },
  solutionPageReleased: { en: 'solution page runtime was released', ko: '해법 페이지 실행 환경이 해제되었습니다.', ja: '解法ページの実行環境を解放しました。' },
  solutionPageDisposed: { en: 'solution page runtime was disposed', ko: '해법 페이지 실행 환경이 폐기되었습니다.', ja: '解法ページの実行環境を破棄しました。' },
  productPageDisposed: { en: 'product page runtime was disposed', ko: '결과 페이지 실행 환경이 폐기되었습니다.', ja: '結果ページの実行環境を破棄しました。' },
  solutionPageMismatch: { en: 'solution page response does not match its request', ko: '해법 페이지 응답이 요청과 일치하지 않습니다.', ja: '解法ページの応答がリクエストと一致しません。' },
  staleProductPage: { en: 'stale product page generation was discarded', ko: '이전 세대의 결과 페이지를 폐기했습니다.', ja: '古い世代の結果ページを破棄しました。' },
  solutionPageAborted: { en: 'Solution page load was aborted.', ko: '해법 페이지 불러오기가 중단되었습니다.', ja: '解法ページの読み込みを中止しました。' },
  productPageAborted: { en: 'Product page load was aborted.', ko: '결과 페이지 불러오기가 중단되었습니다.', ja: '結果ページの読み込みを中止しました。' },
  desktopRequestRequired: { en: 'Desktop requests must be complete clearra-cli/CommandRequest objects', ko: '데스크톱 요청은 완전한 clearra-cli/CommandRequest 객체여야 합니다.', ja: 'デスクトップのリクエストには完全なclearra-cli/CommandRequestオブジェクトが必要です。' },
  desktopArgvRequired: { en: 'Desktop requests require a complete canonical CLI argv envelope', ko: '데스크톱 요청에는 완전한 표준 CLI argv 형식이 필요합니다.', ja: 'デスクトップのリクエストには完全な正規CLI argv形式が必要です。' },
  desktopUnknownField: { en: "Desktop CLI request does not accept field '{field}'", ko: "데스크톱 CLI 요청은 '{field}' 필드를 허용하지 않습니다.", ja: 'デスクトップCLIリクエストではフィールド「{field}」を使用できません。' },
  productPageTimeout: { en: 'Product page work did not return within {timeoutMs} ms.', ko: '결과 페이지 작업이 {timeoutMs}ms 안에 응답하지 않았습니다.', ja: '結果ページの処理が{timeoutMs}ミリ秒以内に応答しませんでした。' },
  preparationTimeout: { en: 'WASM preparation did not complete within {timeoutMs} ms; the worker tree was force-terminated.', ko: 'WASM 준비가 {timeoutMs}ms 안에 완료되지 않아 워커 트리가 강제 종료되었습니다.', ja: 'WASMの準備が{timeoutMs}ミリ秒以内に完了しなかったため、ワーカーツリーを強制終了しました。' },
  searchTimeout: { en: 'The WASM search made no bounded progress for {timeoutMs} ms; the worker tree was force-terminated.', ko: 'WASM 검색이 {timeoutMs}ms 동안 측정 가능한 진행을 보이지 않아 워커 트리가 강제 종료되었습니다.', ja: 'WASM検索で{timeoutMs}ミリ秒間、測定可能な進捗がなかったため、ワーカーツリーを強制終了しました。' },
  terminalFormatFailure: {
    en: 'E_WASM_TERMINAL_FORMAT: Response text could not be formatted. Displaying the terminal again will retry; structured results and diagnostics are unchanged.',
    ko: 'E_WASM_TERMINAL_FORMAT: 응답 텍스트를 표시할 수 없습니다. 터미널을 다시 표시하면 재시도합니다. 구조화된 결과와 진단은 유지됩니다.',
    ja: 'E_WASM_TERMINAL_FORMAT: 応答テキストを整形できませんでした。ターミナルを再表示すると再試行します。構造化された結果と診断は維持されます。'
  }
} as const satisfies Record<string, Record<WorkspaceLocale, string>>;

export type RuntimeShellMessageKey = keyof typeof RUNTIME_SHELL_MESSAGES;

export function runtimeShellCopy(locale: WorkspaceLocale): Record<RuntimeShellMessageKey, string> {
  return Object.fromEntries(
    Object.entries(RUNTIME_SHELL_MESSAGES).map(([key, messages]) => [key, messages[locale]])
  ) as Record<RuntimeShellMessageKey, string>;
}

export function runtimeShellText(
  locale: WorkspaceLocale,
  key: RuntimeShellMessageKey,
  values: Readonly<Record<string, string | number>> = {}
): string {
  return RUNTIME_SHELL_MESSAGES[key][locale].replace(/\{(\w+)\}/gu, (token, name: string) =>
    Object.hasOwn(values, name) ? String(values[name]) : token
  );
}

const knownValues = new Map<string, RuntimeShellMessageKey>(
  Object.entries(RUNTIME_SHELL_MESSAGES)
    .filter(([, messages]) => !messages.en.includes('{'))
    .map(([key, messages]) => [messages.en, key as RuntimeShellMessageKey])
);

/** Only known display text is localized. IDs, hashes and protocol data stay exact. */
export function runtimeShellValue(locale: WorkspaceLocale, value: string | boolean): string {
  const source = String(value);
  const key = knownValues.get(source);
  return key === undefined ? source : runtimeShellText(locale, key);
}

function localizeTerminalLine(locale: WorkspaceLocale, line: string, allowDiagnostic = true): string {
  const exact = knownValues.get(line);
  if (exact !== undefined) return runtimeShellText(locale, exact);
  const started = /^job (\d+) started$/u.exec(line);
  if (started) return runtimeShellText(locale, 'jobStarted', { jobId: started[1] });
  const partial = /^partial: (.*)$/u.exec(line);
  if (partial) return runtimeShellText(locale, 'partialResult', { label: runtimeShellValue(locale, partial[1]) });
  const diagnostic = allowDiagnostic ? /^(E_[A-Z0-9_]+): (.*)$/u.exec(line) : null;
  if (diagnostic) return `${diagnostic[1]}: ${localizeTerminalLine(locale, diagnostic[2], false)}`;
  for (const [pattern, key] of [
    [/^Product page work did not return within (\d+) ms\.$/u, 'productPageTimeout'],
    [/^WASM preparation did not complete within (\d+) ms; the worker tree was force-terminated\.$/u, 'preparationTimeout'],
    [/^The WASM search made no bounded progress for (\d+) ms; the worker tree was force-terminated\.$/u, 'searchTimeout']
  ] as const) {
    const match = pattern.exec(line);
    if (match) return runtimeShellText(locale, key, { timeoutMs: match[1] });
  }
  const unknownField = /^Desktop CLI request does not accept field '([^']*)'$/u.exec(line);
  if (unknownField) return runtimeShellText(locale, 'desktopUnknownField', { field: unknownField[1] });
  return line;
}

export function formatRuntimeShellTranscript(locale: WorkspaceLocale, lines: readonly WasmTerminalLine[]): string {
  return lines.map((line) => localizeTerminalLine(locale, formatWasmTerminalLine(line))).join('\n');
}
