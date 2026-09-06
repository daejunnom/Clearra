export const DEFAULT_DISCORD_LOCALE = "en";
export const SUPPORTED_DISCORD_LOCALES = Object.freeze(["en", "ko"]);

const CATALOGS = Object.freeze({
  en: Object.freeze({
    "preview.searching": "Preview ready. Clearra search is running…",
    "preview.result": "ClearraBot rendered the Clearra result.",
    "preview.document": "Document preview ready.",
    "preview.invalid_attachment": "Clearra could not render the CTK3 attachment because it is invalid or exceeds the preview limits.",
    "preview.invalid_input": "Clearra could not render the Fumen or CTK3 input because it is invalid or exceeds the preview limits.",
    "preview.image_failed": "The GIF preview could not be rendered.",
    "preview.image_limit": "The image preview exceeds the supported size or frame limit.",
    "preview.attachment_description": "Fumen and CTK3 preview",
    "render_file.not_found": "No recent Clearra GIF preview was found in this channel.",
    "render_file.invalid_selection": "The selected message does not contain an available Clearra GIF preview.",
    "render_file.unavailable": "The original GIF is no longer available. Try a newer preview.",
    "render_file.failed": "The original GIF could not be retrieved. Please try again.",
    "render_file.attachment_description": "Original Clearra GIF preview",
    "viewer.open": "Open in Clearra: {url}",
    "viewer.link_too_long": "The direct viewer link exceeds Discord's 2,000-character limit. Open the Clearra CTK renderer and load the attached CTK3 document: {url}",
    "viewer.document_description": "CTK3 document for the Clearra renderer",
    "viewer.document_failed": "The document could not be rendered.",
    "search.stopped": "Clearra search stopped before producing a result.",
    "search.setup_long_running": "The setup search is still running and is taking longer than expected. It will continue within Discord's response window.",
    "search.busy": "Clearra is busy. Please try again shortly.",
    "search.cancelled": "The search was cancelled.",
    "search.timeout": "The calculation exceeded its time limit. Please try again.",
    "search.failed": "The request could not be completed. Please try again.",
    "search.no_text": "Clearra completed without text output.",
    "search.auto_target": "Automatic PC target: {lines}",
    "result.completed": "Clearra {kind} completed{partial}.",
    "result.partial_suffix": " with a partial result",
    "result.ctk3_pages": "CTK3 pages: {count}",
    "result.pc_path_replay_frames": "PC replay: {count} frames at {delay}ms each",
    "result.pc_path_gif_description": "Canonical perfect-clear replay (initial field, locks, and line clears)",
    "result.build_path_replay_frames": "Build replay: {count} frames at {delay}ms each",
    "result.build_path_gif_description": "Canonical build replay (initial field, locks, and line clears)",
    "result.warning": "Warning: {warning}",
    "result.additional_warnings": "Warning: {count} additional incomplete-result conditions.",
    "result.generic_kind": "search",
    "result.kind.pc": "perfect-clear search",
    "result.kind.path": "path search",
    "result.kind.setup": "setup search",
    "result.kind.build": "build-probability search",
    "result.kind.score": "Jstris-score perfect-clear search",
    "result.kind.cover": "coverage search",
    "result.kind.damage": "damage search",
    "result.kind.spin": "spin search",
    "result.kind.spin_structure": "spin-structure search",
    "result.kind.finesse_search": "finesse search",
    "result.kind.finesse_score": "finesse score",
    "result.kind.public.search": "search",
    "result.kind.public.path": "path search",
    "result.kind.public.percent": "perfect-clear probability",
    "result.kind.public.chance": "perfect-clear probability",
    "result.kind.public.minimals": "minimum-cover perfect-clear search",
    "result.kind.public.score": "scored perfect-clear search",
    "result.kind.public.saves": "perfect-clear save groups",
    "result.kind.public.best_save": "best perfect-clear save",
    "result.kind.public.score_minimals": "minimum-cover scored search",
    "result.kind.public.tiling": "perfect-clear tiling search",
    "result.kind.public.failed_queue": "failed-queue search",
    "result.kind.public.cover": "build-coverage search",
    "result.kind.public.probability": "build-probability result aggregation",
    "result.kind.public.spin_cover": "forward spin search",
    "result.kind.public.spin": "forward spin search",
    "result.kind.public.score_finder": "Jstris-score perfect-clear search",
    "result.kind.public.damage": "damage search",
    "result.kind.public.ren": "maximum REN search",
    "result.kind.public.spin_structure": "spin-structure search",
    "result.kind.public.spin_structure_cover": "spin-structure coverage",
    "result.kind.public.spin_structure_guaranteed": "guaranteed spin-structure search",
    "result.kind.public.pc_setup": "joint setup search",
    "result.kind.public.best_setup": "build-first setup search",
    "result.kind.public.dpc_finder": "PC-first setup search",
    "result.kind.public.setup_score": "setup score ranking",
    "result.kind.public.finesse_search": "finesse search",
    "result.kind.public.finesse_score": "finesse score",
    "result.kind.public.allspin_sol": "B2B-preserving PC witness search",
    "result.kind.public.allspin_sol_finder": "B2B-preserving PC witness search",
    "result.kind.public.allspin_pres_chance": "B2B-preserving PC probability",
    "result.kind.public.sequence": "operation trace validation",
    "result.kind.public.sequence_dependencies": "operation-order dependency analysis",
    "result.kind.public.parity": "field-document parity observation",
    "result.kind.public.fumen": "Fumen document transform",
    "result.kind.public.render": "exact field-document render",
    "result.kind.public.to_gray": "occupied-color normalization",
    "result.kind.public.mirror": "mirror transform",
    "result.kind.public.setup": "colored-target build family",
    "result.kind.public.congruent": "colored-target congruence family",
    "result.kind.public.congruent_cover": "congruence coverage portfolio",
    "result.kind.public.setup_cover": "setup coverage portfolio",
    "result.kind.public.setup_cover_percent": "setup coverage probability",
    "result.kind.public.setup_cover_score": "score-only setup coverage portfolio",
    "result.kind.public.evaluate_cover": "supplied-solution coverage family",
    "result.kind.public.evaluate_minimals": "supplied-solution minimum portfolio",
    "result.kind.public.evaluate_score": "supplied-solution score portfolio",
    "result.kind.public.evaluate_b2b_cover": "supplied-solution B2B coverage family",
    "result.kind.public.evaluate_cover_percent": "supplied-solution coverage probability",
    "finesse.exact_total_inputs": "Minimum total",
    "finesse.average.oracle": "Full-queue average",
    "finesse.average.visible_7": "Visible-7 average",
    "finesse.oracle_on_covered_average": "Full-queue average on visible-7 successes",
    "finesse.information_penalty": "Information cost",
    "finesse.success_probability_gap": "Success probability gap",
    "finesse.success_probability": "Success probability",
    "finesse.successful_queues": "Successful queues",
    "finesse.inputs": "{count} inputs",
    "finesse.input": "{count} input",
    "summary.coverage_probability": "Coverage probability",
    "summary.failed_coverage_probability": "Failure probability",
    "summary.covered_pattern_count": "Successful patterns",
    "summary.failed_pattern_count": "Failed patterns",
    "summary.materialized_pattern_count": "Materialized patterns",
    "summary.materialized_probability_mass": "Materialized probability mass",
    "summary.probability_complete": "Probability complete",
    "summary.probability": "Probability",
    "summary.save_group_count": "Save groups",
    "summary.save_pc_probability": "PC probability",
    "summary.best_save_weighted_total": "Weighted total",
    "summary.best_save_balanced_jl_count": "J+L count",
    "summary.best_save_exact_group_probability": "Exact group probability",
    "summary.selected_result": "Selected result",
    "summary.weighted_probability": "Weighted probability",
    "summary.total_solution_count": "Solutions",
    "summary.unique_solution_count": "Unique solutions",
    "summary.normalized_unique_solution_count": "Normalized solutions",
    "summary.result_count": "Results",
    "summary.regular_count": "Regular structures",
    "summary.mini_count": "Mini structures",
    "summary.minimum_placements": "Minimum placements",
    "summary.maximum_damage": "Maximum damage",
    "summary.maximum_ren": "Maximum REN",
    "summary.operation_count": "Operations",
    "summary.cleared_line_count": "Cleared lines",
    "summary.rule_profile": "Rule profile",
    "summary.kick_profile": "Kick profile",
    "summary.exact_order_count": "Exact accepted orders",
    "summary.universal_dependency_count": "Universal dependencies",
    "summary.transitive_reduction_count": "Transitive-reduction edges",
    "summary.independent_pair_count": "Independent operation pairs",
    "summary.score_solution_field_count": "Solution fields",
    "summary.score_success_pattern_count": "Successful PC patterns",
    "summary.score_failed_pc_pattern_count": "Failed PC patterns",
    "summary.score_covered_probability": "PC coverage",
    "summary.score_overall_score": "Overall score",
    "summary.score_covered_pattern_conditional_average_score": "Covered-pattern average score",
    "summary.score_summary_complete": "Score summary complete",
    "summary.pc_allspin_spin_profile": "Spin profile",
    "summary.pc_allspin_preserving_queue_count": "B2B-preserving queues",
    "summary.pc_allspin_original_queue_count": "Original queue universe",
    "summary.pc_allspin_preservation_probability": "B2B-preservation probability",
    "summary.pc_allspin_preserves_b2b": "Preserves B2B",
    "summary.pc_allspin_witness_available": "Witness available",
    "summary.pc_allspin_count_complete": "Queue count complete",
    "summary.pc_allspin_probability_complete": "Probability complete",
    "summary.boolean_true": "Yes",
    "summary.boolean_false": "No",
    "summary.solution_count_not_calculated": "Solution count: Not calculated",
    "warning.incomplete": "Some results may be incomplete.",
    "warning.truncated": "Some results were omitted because a result limit was reached.",
    "warning.tiling_only": "WARNING: Buildability and probability are skipped. Results may include solutions that cannot be built.",
    "result.title": "Clearra result:",
    "result.output_description": "Clearra command output",
    "result.ctk3_description": "Complete color-preserving Clearra CTK3 result",
    "error.request": "Clearra could not complete the request: {message}",
    "error.validation": "Check the command input and try again.",
    "error.form": "Clearra could not open the input form. Please try again.",
    "error.unsupported_command": "Only supported ClearraBot commands are enabled.",
    "error.result_consistency": "Clearra returned an inconsistent result. Please retry the command.",
    "input.pieces.invalid": "pieces must contain only IOTSZJL tetrominoes.",
    "input.profile.invalid": "profile must be T-Spins, T-Spins+, All-Mini(+), or All-Spin(+).",
    "input.document.invalid": "document must be a valid CTK3 or v115 Fumen with one placement operation on every page.",
    "input.options.setup_qb_required": "setup mode=qb requires a qb piece inventory.",
    "input.options.setup_qb_oracle_conflict": "qb pieces cannot be used with setup mode=oracle.",
    "input.options.setup_qb_bag_capacity": "setup qb pieces and remaining pieces must fit within one seven-piece bag.",
    "input.options.setup_borrow_cycle": "post-cycle borrowing is available only when remaining represents cycle 7 (three pieces).",
    "input.options.modal_unrepresented": "The guided form cannot preserve {options}; provide every required board and queue field directly with those options.",
    "input.options.spin_fill_bounds": "fill-bottom must be lower than fill-top.",
    "input.options.finesse_score_unsupported": "finesse score does not support aggregation, spin-profile, or preserve-b2b.",
    "input.options.finesse_spin_dependency": "spin-profile requires aggregation=spin or preserve-b2b=on.",
    "input.options.invalid": "The value for {option} is invalid.",
    "input.source.invalid": "The queue or pattern source is invalid.",
    "security.rate_limited": "Too many requests were sent from this account. Please wait briefly and try again.",
    "security.repeated_request": "The same request was submitted too quickly. Please wait briefly before trying it again.",
    "access.guild_paused": "Clearra commands are temporarily unavailable in this server.",
    "access.channel_disabled": "Clearra commands are unavailable in this channel.",
    "management.guild_only": "This setting can be changed only inside a server.",
    "management.permission.channel": "Manage Channels permission or ClearraBot administrator access is required.",
    "management.permission.guild": "Manage Server permission or ClearraBot administrator access is required.",
    "management.channel.disabled": "Clearra commands are now disabled in this channel.",
    "management.channel.enabled": "Clearra commands are now enabled in this channel.",
    "management.guild.paused": "Clearra commands are now paused in this server. Only server resume remains available.",
    "management.guild.resumed": "Clearra commands are now available in this server.",
    "language.current": "Effective {scope} language: {language} ({source}).",
    "language.updated": "The {scope} default language is now {language}.",
    "language.reset": "The {scope} language override was removed. The effective language is {language}.",
    "language.scope.channel": "channel",
    "language.scope.guild": "server",
    "language.source.channel": "channel setting",
    "language.source.guild": "server setting",
    "language.source.global": "global default",
    "language.name.en": "English",
    "language.name.ko": "Korean",
    "language.guild_only": "Language settings can be changed only inside a server.",
    "language.permission.channel": "Manage Channels permission or ClearraBot administrator access is required to change the channel language.",
    "language.permission.guild": "Manage Server permission or ClearraBot administrator access is required to change the server language.",
  }),
  ko: Object.freeze({
    "preview.searching": "미리보기가 준비되었습니다. Clearra 탐색을 진행하고 있습니다…",
    "preview.result": "Clearra 결과 미리보기가 준비되었습니다.",
    "preview.document": "문서 미리보기가 준비되었습니다.",
    "preview.invalid_attachment": "CTK3 첨부 파일이 올바르지 않거나 미리보기 제한을 넘어 렌더링할 수 없습니다.",
    "preview.invalid_input": "Fumen 또는 CTK3 입력이 올바르지 않거나 미리보기 제한을 넘어 렌더링할 수 없습니다.",
    "preview.image_failed": "이미지 미리보기를 렌더링할 수 없습니다.",
    "preview.image_limit": "이미지 미리보기가 지원되는 크기 또는 프레임 제한을 넘습니다.",
    "preview.attachment_description": "Fumen 및 CTK3 미리보기",
    "render_file.not_found": "이 채널에서 최근 Clearra GIF 미리보기를 찾지 못했습니다.",
    "render_file.invalid_selection": "선택한 메시지에 사용할 수 있는 Clearra GIF 미리보기가 없습니다.",
    "render_file.unavailable": "원본 GIF를 더 이상 사용할 수 없습니다. 더 최근 미리보기를 선택해 주세요.",
    "render_file.failed": "원본 GIF를 가져오지 못했습니다. 다시 시도해 주세요.",
    "render_file.attachment_description": "Clearra 원본 GIF 미리보기",
    "viewer.open": "Clearra에서 열기: {url}",
    "viewer.link_too_long": "바로 열기 링크가 Discord의 2,000자 제한을 넘습니다. Clearra CTK 렌더러를 열고 첨부된 CTK3 문서를 불러오세요: {url}",
    "viewer.document_description": "Clearra 렌더러용 CTK3 문서",
    "viewer.document_failed": "문서를 렌더링할 수 없습니다.",
    "search.stopped": "결과를 만들기 전에 Clearra 탐색이 중단되었습니다.",
    "search.setup_long_running": "셋업 탐색이 아직 진행 중이며 예상보다 오래 걸리고 있습니다. Discord 응답 제한 안에서 계속 탐색합니다.",
    "search.busy": "요청이 많습니다. 잠시 후 다시 시도해 주세요.",
    "search.cancelled": "탐색이 취소되었습니다.",
    "search.timeout": "제한 시간 안에 연산을 마치지 못했습니다. 다시 시도해 주세요.",
    "search.failed": "요청을 완료할 수 없습니다. 다시 시도해 주세요.",
    "search.no_text": "Clearra가 텍스트 출력 없이 작업을 완료했습니다.",
    "search.auto_target": "자동 PC 목표: {lines}",
    "result.completed": "Clearra {kind}을(를) 완료했습니다{partial}.",
    "result.partial_suffix": "(일부 결과)",
    "result.ctk3_pages": "CTK3 페이지: {count}",
    "result.pc_path_replay_frames": "PC 리플레이: {count}프레임 · 프레임당 {delay}ms",
    "result.pc_path_gif_description": "정규 PC 대표 리플레이(초기 필드, 배치, 라인 클리어)",
    "result.build_path_replay_frames": "구축 리플레이: {count}프레임 · 프레임당 {delay}ms",
    "result.build_path_gif_description": "정규 구축 대표 리플레이(초기 필드, 배치, 라인 클리어)",
    "result.warning": "주의: {warning}",
    "result.additional_warnings": "주의: 불완전한 결과 조건이 {count}개 더 있습니다.",
    "result.generic_kind": "탐색",
    "result.kind.pc": "퍼펙트 클리어 탐색",
    "result.kind.path": "경로 탐색",
    "result.kind.setup": "셋업 탐색",
    "result.kind.build": "구축 확률 탐색",
    "result.kind.score": "Jstris 점수 퍼펙트 클리어 탐색",
    "result.kind.cover": "커버리지 탐색",
    "result.kind.damage": "대미지 탐색",
    "result.kind.spin": "스핀 탐색",
    "result.kind.spin_structure": "스핀 구조 탐색",
    "result.kind.finesse_search": "피네스 탐색",
    "result.kind.finesse_score": "피네스 계산",
    "result.kind.public.search": "탐색",
    "result.kind.public.path": "경로 탐색",
    "result.kind.public.percent": "퍼펙트 클리어 확률 계산",
    "result.kind.public.chance": "퍼펙트 클리어 확률 계산",
    "result.kind.public.minimals": "최소 커버 퍼펙트 클리어 탐색",
    "result.kind.public.score": "점수 퍼펙트 클리어 탐색",
    "result.kind.public.saves": "퍼펙트 클리어 세이브 그룹",
    "result.kind.public.best_save": "최적 퍼펙트 클리어 세이브",
    "result.kind.public.score_minimals": "최소 커버 점수 탐색",
    "result.kind.public.tiling": "퍼펙트 클리어 타일링 탐색",
    "result.kind.public.failed_queue": "실패 큐 탐색",
    "result.kind.public.cover": "구축 커버리지 탐색",
    "result.kind.public.probability": "구축 확률 결과 집계",
    "result.kind.public.spin_cover": "정방향 스핀 탐색",
    "result.kind.public.spin": "정방향 스핀 탐색",
    "result.kind.public.score_finder": "Jstris 점수 퍼펙트 클리어 탐색",
    "result.kind.public.damage": "대미지 탐색",
    "result.kind.public.ren": "최대 REN 탐색",
    "result.kind.public.spin_structure": "스핀 구조 탐색",
    "result.kind.public.spin_structure_cover": "스핀 구조 커버리지",
    "result.kind.public.spin_structure_guaranteed": "보장 스핀 구조 탐색",
    "result.kind.public.pc_setup": "종합 셋업 탐색",
    "result.kind.public.best_setup": "구축 우선 셋업 탐색",
    "result.kind.public.dpc_finder": "PC 우선 셋업 탐색",
    "result.kind.public.setup_score": "셋업 점수 순위 계산",
    "result.kind.public.finesse_search": "피네스 탐색",
    "result.kind.public.finesse_score": "피네스 계산",
    "result.kind.public.allspin_sol": "B2B 보존 PC 증거 탐색",
    "result.kind.public.allspin_sol_finder": "B2B 보존 PC 증거 탐색",
    "result.kind.public.allspin_pres_chance": "B2B 보존 PC 확률 계산",
    "result.kind.public.sequence": "operation trace 유효성 확인",
    "result.kind.public.sequence_dependencies": "operation 순서 의존성 분석",
    "result.kind.public.parity": "field-document 패리티 관찰",
    "result.kind.public.fumen": "Fumen 문서 변환",
    "result.kind.public.render": "정확한 field-document 렌더",
    "result.kind.public.to_gray": "점유 색상 회색화",
    "result.kind.public.mirror": "좌우 반전",
    "result.kind.public.setup": "색상 목표 구축 패밀리",
    "result.kind.public.congruent": "색상 목표 합동 패밀리",
    "result.kind.public.congruent_cover": "합동 커버리지 포트폴리오",
    "result.kind.public.setup_cover": "셋업 커버리지 포트폴리오",
    "result.kind.public.setup_cover_percent": "셋업 커버리지 확률",
    "result.kind.public.setup_cover_score": "점수 전용 셋업 커버리지 포트폴리오",
    "result.kind.public.evaluate_cover": "제공 해법 커버리지 패밀리",
    "result.kind.public.evaluate_minimals": "제공 해법 최소 포트폴리오",
    "result.kind.public.evaluate_score": "제공 해법 점수 포트폴리오",
    "result.kind.public.evaluate_b2b_cover": "제공 해법 B2B 커버리지 패밀리",
    "result.kind.public.evaluate_cover_percent": "제공 해법 커버리지 확률",
    "finesse.exact_total_inputs": "최소 총 입력",
    "finesse.average.oracle": "전체 큐 평균",
    "finesse.average.visible_7": "공개 7개 평균",
    "finesse.oracle_on_covered_average": "공개 7개 성공 큐의 전체 큐 평균",
    "finesse.information_penalty": "정보 비용",
    "finesse.success_probability_gap": "성공 확률 차이",
    "finesse.success_probability": "성공 확률",
    "finesse.successful_queues": "성공 큐",
    "finesse.inputs": "{count}입력",
    "finesse.input": "{count}입력",
    "summary.coverage_probability": "커버 확률",
    "summary.failed_coverage_probability": "실패 확률",
    "summary.covered_pattern_count": "성공 패턴",
    "summary.failed_pattern_count": "실패 패턴",
    "summary.materialized_pattern_count": "구체화 패턴",
    "summary.materialized_probability_mass": "구체화 확률 질량",
    "summary.probability_complete": "확률 완전성",
    "summary.probability": "확률",
    "summary.save_group_count": "세이브 그룹",
    "summary.save_pc_probability": "PC 확률",
    "summary.best_save_weighted_total": "가중 합계",
    "summary.best_save_balanced_jl_count": "J+L 개수",
    "summary.best_save_exact_group_probability": "정확 그룹 확률",
    "summary.selected_result": "선택된 결과",
    "summary.weighted_probability": "가중 확률",
    "summary.total_solution_count": "해법",
    "summary.unique_solution_count": "고유 해법",
    "summary.normalized_unique_solution_count": "정규화 해법",
    "summary.result_count": "결과",
    "summary.regular_count": "Regular 구조",
    "summary.mini_count": "Mini 구조",
    "summary.minimum_placements": "최소 배치 수",
    "summary.maximum_damage": "최대 대미지",
    "summary.maximum_ren": "최대 REN",
    "summary.operation_count": "Operation 수",
    "summary.cleared_line_count": "삭제한 줄 수",
    "summary.rule_profile": "규칙 프로필",
    "summary.kick_profile": "킥 프로필",
    "summary.exact_order_count": "정확한 허용 순서 수",
    "summary.universal_dependency_count": "보편 선행 관계 수",
    "summary.transitive_reduction_count": "전이 축약 간선 수",
    "summary.independent_pair_count": "독립 operation 쌍 수",
    "summary.score_solution_field_count": "해법 필드 수",
    "summary.score_success_pattern_count": "PC 성공 패턴 수",
    "summary.score_failed_pc_pattern_count": "PC 실패 패턴 수",
    "summary.score_covered_probability": "PC 커버리지",
    "summary.score_overall_score": "전체 점수",
    "summary.score_covered_pattern_conditional_average_score": "커버된 패턴 평균 점수",
    "summary.score_summary_complete": "점수 요약 완전성",
    "summary.pc_allspin_spin_profile": "스핀 프로필",
    "summary.pc_allspin_preserving_queue_count": "B2B 보존 큐",
    "summary.pc_allspin_original_queue_count": "원래 큐 전체집합",
    "summary.pc_allspin_preservation_probability": "B2B 보존 확률",
    "summary.pc_allspin_preserves_b2b": "B2B 보존",
    "summary.pc_allspin_witness_available": "증거 사용 가능",
    "summary.pc_allspin_count_complete": "큐 개수 완전성",
    "summary.pc_allspin_probability_complete": "확률 완전성",
    "summary.boolean_true": "예",
    "summary.boolean_false": "아니요",
    "summary.solution_count_not_calculated": "해법 수: 계산하지 않음",
    "warning.incomplete": "일부 결과가 불완전할 수 있습니다.",
    "warning.truncated": "결과 제한에 도달해 일부 결과가 생략되었습니다.",
    "warning.tiling_only": "주의: 구축 가능성과 확률 계산을 생략했습니다. 구축할 수 없는 해법이 포함될 수 있습니다.",
    "result.title": "Clearra 결과:",
    "result.output_description": "Clearra 명령어 출력",
    "result.ctk3_description": "색상을 보존한 전체 Clearra CTK3 결과",
    "error.request": "Clearra가 요청을 완료하지 못했습니다: {message}",
    "error.validation": "명령어 입력을 확인한 뒤 다시 시도해 주세요.",
    "error.form": "입력 창을 열 수 없습니다. 다시 시도해 주세요.",
    "error.unsupported_command": "지원되는 ClearraBot 명령어만 사용할 수 있습니다.",
    "error.result_consistency": "Clearra 결과가 요청한 명령과 일치하지 않습니다. 명령어를 다시 실행해 주세요.",
    "input.pieces.invalid": "미노 목록에는 IOTSZJL만 입력해 주세요.",
    "input.profile.invalid": "프로필은 T-Spins, T-Spins+, All-Mini(+), All-Spin(+) 중 하나여야 합니다.",
    "input.document.invalid": "문서는 각 페이지에 배치 operation이 하나씩 있는 올바른 CTK3 또는 v115 Fumen이어야 합니다.",
    "input.options.setup_qb_required": "셋업 mode=qb에는 qb 미노 목록이 필요합니다.",
    "input.options.setup_qb_oracle_conflict": "셋업 mode=oracle에서는 qb 미노를 사용할 수 없습니다.",
    "input.options.setup_qb_bag_capacity": "셋업 qb 미노와 remaining 미노는 하나의 7개 미노 백 안에 들어가야 합니다.",
    "input.options.setup_borrow_cycle": "회차 이후 빌리기는 remaining이 7회차를 나타내는 미노 3개일 때만 사용할 수 있습니다.",
    "input.options.modal_unrepresented": "입력 창에서는 {options} 옵션을 보존할 수 없습니다. 해당 옵션을 사용하려면 필수 필드와 큐를 슬래시 명령어에 직접 입력해 주세요.",
    "input.options.spin_fill_bounds": "fill-bottom은 fill-top보다 작아야 합니다.",
    "input.options.finesse_score_unsupported": "피네스 계산에서는 aggregation, spin-profile, preserve-b2b를 사용할 수 없습니다.",
    "input.options.finesse_spin_dependency": "spin-profile에는 aggregation=spin 또는 preserve-b2b=on이 필요합니다.",
    "input.options.invalid": "{option} 옵션 값이 올바르지 않습니다.",
    "input.source.invalid": "큐 또는 패턴 소스가 올바르지 않습니다.",
    "security.rate_limited": "이 계정에서 요청을 너무 많이 보냈습니다. 잠시 후 다시 시도해 주세요.",
    "security.repeated_request": "같은 요청이 너무 빠르게 반복되었습니다. 잠시 후 다시 시도해 주세요.",
    "access.guild_paused": "이 서버에서는 Clearra 명령을 일시적으로 사용할 수 없습니다.",
    "access.channel_disabled": "이 채널에서는 Clearra 명령을 사용할 수 없습니다.",
    "management.guild_only": "이 설정은 서버 안에서만 변경할 수 있습니다.",
    "management.permission.channel": "채널 관리 권한 또는 ClearraBot 관리자 권한이 필요합니다.",
    "management.permission.guild": "서버 관리 권한 또는 ClearraBot 관리자 권한이 필요합니다.",
    "management.channel.disabled": "이 채널에서 Clearra 명령을 비활성화했습니다.",
    "management.channel.enabled": "이 채널에서 Clearra 명령을 다시 활성화했습니다.",
    "management.guild.paused": "이 서버의 Clearra 명령을 일시정지했습니다. 서버 재개만 사용할 수 있습니다.",
    "management.guild.resumed": "이 서버에서 Clearra 명령을 다시 사용할 수 있습니다.",
    "language.current": "현재 {scope} 적용 언어: {language} ({source}).",
    "language.updated": "{scope} 기본 언어를 {language}(으)로 변경했습니다.",
    "language.reset": "{scope} 언어 설정을 삭제했습니다. 현재 적용 언어는 {language}입니다.",
    "language.scope.channel": "채널",
    "language.scope.guild": "서버",
    "language.source.channel": "채널 설정",
    "language.source.guild": "서버 설정",
    "language.source.global": "전체 기본값",
    "language.name.en": "영어",
    "language.name.ko": "한국어",
    "language.guild_only": "언어 설정은 서버 안에서만 변경할 수 있습니다.",
    "language.permission.channel": "채널 언어를 변경하려면 채널 관리 권한 또는 ClearraBot 관리자 권한이 필요합니다.",
    "language.permission.guild": "서버 언어를 변경하려면 서버 관리 권한 또는 ClearraBot 관리자 권한이 필요합니다.",
  }),
});

export function normalizeDiscordLocale(value, fallback = DEFAULT_DISCORD_LOCALE) {
  const normalizedFallback = matchDiscordLocale(fallback) ?? DEFAULT_DISCORD_LOCALE;
  return matchDiscordLocale(value) ?? normalizedFallback;
}

export function matchDiscordLocale(value) {
  if (typeof value !== "string") return null;
  const normalized = value.trim().toLowerCase().replaceAll("_", "-");
  if (normalized === "ko" || normalized.startsWith("ko-")) return "ko";
  if (normalized === "en" || normalized.startsWith("en-")) return "en";
  return null;
}

export function isSupportedDiscordLocale(value) {
  return supportedLocale(value) !== null;
}

export function t(locale, key, values = {}) {
  const language = normalizeDiscordLocale(locale);
  const template = CATALOGS[language][key] ?? CATALOGS.en[key];
  if (typeof template !== "string") throw new Error(`Unknown Discord translation key '${key}'.`);
  return template.replace(/\{([a-z0-9_]+)\}/gi, (_match, name) =>
    Object.hasOwn(values, name) ? String(values[name]) : `{${name}}`
  );
}

export class DiscordInputError extends Error {
  constructor(code, details = {}, message = `Discord input validation failed: ${code}`) {
    super(message);
    this.name = "DiscordInputError";
    this.code = code;
    this.details = Object.freeze({ ...details });
  }
}

export function validationErrorText(error, locale) {
  if (error instanceof DiscordInputError) {
    const language = normalizeDiscordLocale(locale);
    return t(language, "error.request", {
      message: t(language, `input.${error.code}`, error.details),
    });
  }
  const message = error instanceof Error ? error.message : String(error ?? "");
  if (normalizeDiscordLocale(locale) === "en") {
    return t("en", "error.request", { message: safePublicValidationMessage(message, "en") });
  }
  return t("ko", "error.request", {
    message: koreanValidationMessage(message),
  });
}

export function operationErrorText(error, locale) {
  const message = error instanceof Error ? error.message : String(error ?? "");
  let key = "search.failed";
  if (error?.name === "AbortError" || /cancel(?:led|ed)/i.test(message)) {
    key = "search.cancelled";
  } else if (/time(?:d)? out|time limit|deadline/i.test(message)) {
    key = "search.timeout";
  } else if (/\bbusy\b|concurrency limit|\b429\b|\b503\b/i.test(message)) {
    key = "search.busy";
  }
  return t(locale, key);
}

export function modalErrorText(locale) {
  return t(locale, "error.form");
}

export function assertDiscordCatalogComplete() {
  const english = Object.keys(CATALOGS.en).sort();
  for (const locale of SUPPORTED_DISCORD_LOCALES) {
    const keys = Object.keys(CATALOGS[locale]).sort();
    if (keys.length !== english.length || keys.some((key, index) => key !== english[index])) {
      throw new Error(`Discord translation catalog '${locale}' is incomplete.`);
    }
  }
  return true;
}

function supportedLocale(value) {
  return typeof value === "string" && SUPPORTED_DISCORD_LOCALES.includes(value)
    ? value
    : null;
}

function safePublicValidationMessage(message, locale) {
  if (!message || containsInternalSurface(message)) return t(locale, "error.validation");
  return message.slice(0, 1200);
}

function koreanValidationMessage(message) {
  const exact = KOREAN_VALIDATION_MESSAGES.get(message);
  if (exact) return exact;
  for (const [pattern, replacement] of KOREAN_VALIDATION_PATTERNS) {
    const match = pattern.exec(message);
    if (match) return replacement(...match.slice(1));
  }
  return t("ko", "error.validation");
}

function containsInternalSurface(message) {
  return /(oracle|cloud\s*run|gateway|job service|job id|job state|protocol|endpoint|authorization|token|worker|logical processor|process signal|exit code|runtime|vCPU|OCI|vault|engine|calculation server|schema|catalog|contract|allocation|policy|out of bounds|component IDs?|(?:field|candidate|problem|pattern|operation|group|schema)[_\s-]*IDs?|canonical candidate|(?:normalized[_\s-]*)?trace(?:[_\s-]*(?:identity|key))?|ctk1|Discord supplied|Discord (?:command )?(?:input )?form|Discord Modal|Modal (?:input|select|component|limit)|payload URL|cannot be represented|received an invalid|\bE(?:ACCES|PERM|NOENT|IO)\b|\bsyscall\b|\b(?:backend|tablebase|Web?GPU|WASM)\b|node_modules|\bat\s+file:|(?:^|\s)[A-Za-z]:[\\/]|\/(?:home|root|var|tmp|opt|workspace)\/)/i.test(message);
}

const KOREAN_VALIDATION_MESSAGES = new Map([
  ["Enter a Clearra command.", "Clearra 명령어를 입력해 주세요."],
  ["The command has too many arguments.", "명령어 인수가 너무 많습니다."],
  ["The command is too long.", "명령어가 너무 깁니다."],
  ["The command contains an unterminated quote.", "명령어의 따옴표가 닫히지 않았습니다."],
  ["File and custom-code inputs are not available through Discord.", "Discord에서는 파일 경로와 사용자 코드 입력을 사용할 수 없습니다."],
  ["target must contain at least one occupied cell.", "목표 필드에는 블록이 하나 이상 있어야 합니다."],
  ["base and target must not overlap; target contains only cells to add.", "기존 필드와 목표 필드는 겹칠 수 없습니다. 목표 필드에는 추가할 블록만 입력해 주세요."],
  ["target occupied-cell count must be divisible by four.", "목표 필드의 블록 수는 4의 배수여야 합니다."],
  ["base must not contain an already completed row.", "기존 필드에는 이미 완성된 줄이 없어야 합니다."],
  ["next must be a queue or pattern, not a command-line option.", "넥스트에는 명령어 옵션이 아닌 큐 또는 패턴을 입력해 주세요."],
  ["next must be an exact queue containing only IOTSZJL pieces.", "넥스트에는 IOTSZJL만 사용한 정확한 큐를 입력해 주세요."],
  ["field URL is invalid.", "필드 URL이 올바르지 않습니다."],
  ["field URL must contain exactly one CTK3 or Fumen document.", "필드 URL에는 CTK3 또는 Fumen 문서가 정확히 하나 있어야 합니다."],
  ["field document cannot be empty.", "필드 문서는 비워 둘 수 없습니다."],
  ["field must contain one raw CTK3 or v115 Fumen document.", "필드에는 CTK3 또는 v115 Fumen 문서 하나를 입력해 주세요."],
  ["v110 Fumen is not supported by the Clearra search decoder; use v115.", "Clearra 탐색에서는 v110 Fumen을 지원하지 않습니다. v115를 사용해 주세요."],
  ["CTK3 search fields must be exactly 10 columns wide.", "CTK3 탐색 필드는 정확히 10열이어야 합니다."],
  ["CTK3 field could not be decoded.", "CTK3 필드를 해석할 수 없습니다."],
  ["next pattern alternatives must have the same piece count.", "넥스트 패턴의 각 대안은 미노 수가 같아야 합니다."],
  ["next pattern must contain at least one piece.", "넥스트 패턴에는 미노가 하나 이상 있어야 합니다."],
  ["next '*' must be followed immediately by ! or pN.", "넥스트의 `*` 뒤에는 공백 없이 `!` 또는 `pN`이 와야 합니다."],
  ["next pattern contains an empty alternative.", "넥스트 패턴에는 빈 대안을 사용할 수 없습니다."],
  ["next '*' must be followed by ! or pN.", "넥스트의 `*` 뒤에는 `!` 또는 `pN`이 와야 합니다."],
  ["next standard-bag draws may not exceed seven pieces per group.", "넥스트 표준 가방의 각 그룹에서는 7개를 초과해 뽑을 수 없습니다."],
  ["next pattern has an unterminated piece group.", "넥스트 패턴의 미노 그룹이 닫히지 않았습니다."],
  ["next pattern contains an invalid piece group.", "넥스트 패턴의 미노 그룹이 올바르지 않습니다."],
  ["next pattern piece group must leave at least one choice.", "넥스트 패턴의 미노 그룹에는 선택 가능한 미노가 하나 이상 남아야 합니다."],
  ["next pattern has an unexpected bag token after a piece group.", "넥스트 패턴의 미노 그룹 뒤에 예상하지 못한 가방 토큰이 있습니다."],
  ["next pattern draws more pieces than its group contains.", "넥스트 패턴은 그룹에 포함된 미노 수보다 많이 뽑을 수 없습니다."],
  ["next pattern draw count is missing.", "넥스트 패턴의 뽑기 개수를 입력해 주세요."],
  ["next pattern draw count must be a positive integer.", "넥스트 패턴의 뽑기 개수는 양의 정수여야 합니다."],
  ["lines and legacy options clear/lines may not be specified together.", "줄 입력과 기존 options의 clear/lines 설정을 동시에 사용할 수 없습니다."],
  ["options clear must be an integer from 1 through 6.", "기존 options의 clear 값은 1부터 6까지의 정수여야 합니다."],
  ["options type must be TSS, TSD, TST, TSPIN, T-SPIN, or ANY; TSM is unavailable.", "options의 type은 TSS, TSD, TST, TSPIN, T-SPIN, ANY 중 하나여야 하며 TSM은 지원하지 않습니다."],
  ["--arguments requires exactly one command name.", "/help의 --arguments에는 명령어 이름을 정확히 하나 입력해야 합니다."],
  ["Text command /help accepts at most one command name.", "텍스트 /help에는 명령어 이름을 하나만 입력할 수 있습니다."],
  ["help arguments cannot be empty.", "/help의 명령어 인수는 비워 둘 수 없습니다."],
  ["help arguments exceeds the 64-character limit.", "/help의 명령어 인수는 64자를 넘을 수 없습니다."],
  ["The command contains an unterminated code block.", "명령어의 코드 블록이 닫히지 않았습니다."],
  ["A command code block cannot be empty.", "명령어 코드 블록은 비워 둘 수 없습니다."],
  ["remaining must contain only IOTSZJL pieces.", "남은 미노에는 IOTSZJL만 사용할 수 있습니다."],
  ["priority must be all, build, or pc.", "셋업 정렬은 all, build, pc 중 하나여야 합니다."],
  ["queue-knowledge must be full-queue or visible-7.", "큐 공개 범위는 full-queue 또는 visible-7이어야 합니다."],
  ["setup-length must be auto, longer, or shorter.", "셋업 길이는 auto, longer, shorter 중 하나여야 합니다."],
  ["When next-cycle-remaining or setup-length is set, remaining must also be supplied directly.", "다음 회차 남은 미노 또는 셋업 길이를 먼저 설정했다면 남은 미노도 슬래시 명령어에 직접 입력해 주세요."],
]);

const KOREAN_VALIDATION_PATTERNS = Object.freeze([
  [/^(.+) is required in the Clearra command Modal\.$/, (name) => `${koreanInputName(name)} 입력은 필수입니다.`],
  [/^\/(.+) input is required\.$/, (name) => `${koreanInputName(name)} 입력은 필수입니다.`],
  [/^(.+) cannot be empty\.$/, (name) => `${koreanInputName(name)} 입력은 비워 둘 수 없습니다.`],
  [/^(.+) must be text\.$/, (name) => `${koreanInputName(name)} 입력은 텍스트여야 합니다.`],
  [/^(.+) exceeds the (\d+)-character limit\.$/, (name, limit) => `${koreanInputName(name)} 입력은 ${limit}자를 넘을 수 없습니다.`],
  [/^(.+) must be an integer from (\d+) through (\d+)\.$/, (name, min, max) => `${koreanInputName(name)} 값은 ${min}부터 ${max}까지의 정수여야 합니다.`],
  [/^(.+) must contain from 1 through 7 pieces\.$/, (name) => `${koreanInputName(name)}에는 미노를 1–7개 입력해야 합니다.`],
  [/^(.+) must contain only IOTSZJL pieces\.$/, (name) => `${koreanInputName(name)}에는 IOTSZJL만 사용할 수 있습니다.`],
  [/^(.+) allows at most one piece kind twice; no piece may appear three times\.$/, (name) => `${koreanInputName(name)}에서는 한 종류만 두 번 사용할 수 있고 같은 미노를 세 번 사용할 수 없습니다.`],
  [/^next-cycle-remaining must contain exactly (\d+) pieces? when remaining contains (\d+)\.$/, (expected, remaining) => `남은 미노가 ${remaining}개이면 다음 회차 남은 미노는 정확히 ${expected}개여야 합니다.`],
  [/^(.+) grid rows must be exactly 10 columns wide\.$/, (name) => `${koreanInputName(name)} 격자의 각 줄은 정확히 10칸이어야 합니다.`],
  [
    /^(.+) grid must contain from one through (six|twenty-four) rows\.$/,
    (name, maximum) =>
      `${koreanInputName(name)} 격자는 1–${maximum === "six" ? "6" : "24"}줄이어야 합니다.`,
  ],
  [/^(.+) CTK3 must contain exactly one page\.$/, (name) => `${koreanInputName(name)} CTK3에는 페이지가 정확히 하나 있어야 합니다.`],
  [/^(.+) CTK3 exceeds the (\d+)-row limit\.$/, (name, rows) => `${koreanInputName(name)} CTK3는 ${rows}줄을 넘을 수 없습니다.`],
  [/^(.+) Fumen could not be decoded\.$/, (name) => `${koreanInputName(name)} Fumen을 해석할 수 없습니다.`],
  [/^(.+) Fumen must contain exactly one page\.$/, (name) => `${koreanInputName(name)} Fumen에는 페이지가 정확히 하나 있어야 합니다.`],
  [/^(.+) requires a value\.$/, (name) => `${koreanInputName(name)} 값을 입력해 주세요.`],
]);

const KOREAN_INPUT_NAMES = Object.freeze({
  arguments: "명령어",
  image: "이미지",
  field: "필드",
  base: "기존 필드",
  target: "목표 필드",
  next: "넥스트",
  lines: "줄",
  kicktable: "킥테이블",
  options: "옵션",
  remaining: "남은 미노",
  priority: "셋업 정렬",
  "max-setup-pieces": "최대 구축 미노",
  "queue-knowledge": "큐 공개 범위",
  "next-cycle-remaining": "다음 회차 남은 미노",
  "setup-length": "셋업 길이",
  scope: "범위",
});

function koreanInputName(name) {
  return KOREAN_INPUT_NAMES[String(name).trim().toLowerCase()] ?? name;
}
