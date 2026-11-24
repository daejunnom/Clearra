# 고정 필드 GUI 옵션과 최적화 적용 범위

기준 검토·벤치 소스: `49fd03da6255c024f71c92d21b305fe34ec99b94` · 검토일: 2026-11-24

이 문서는 GUI 입력이 어떤 제품과 실행 소유자에 도달하는지, 기존 최적화와 이번 변경이 어디에 적용되는지, 실제 실행으로 확인한 범위를 정리한다. **모든 GUI 옵션 범주를 검토했지만, 가능한 모든 입력과 옵션의 전체 조합을 실행한 것은 아니다.** 성능 수치는 별도의 동일 커밋 벤치마크 보고서에서 다룬다. Full 6L은 보류 상태이며 이 검증에 포함하지 않았다.

**최종 채택 결정:** 비계측 실행 각 2회의 평균은 cutoff=0 **147.996216817초**, cutoff=1 **152.880381248초**로 cutoff가 **3.30% 느렸다**. 따라서 최소 지지 수 cutoff는 제품 기본값으로 채택하지 않고 전체 스캔으로 복귀했다. cutoff 구현과 정확성 검사는 명시적 opt-in 실험으로 유지한다. 아래의 기존 실행 검증 수와 원시 벤치 자료는 기준 커밋의 결과이며, 기본값 복귀 후의 후속 회귀 검증과 구분한다. D0/D1 계측 시간은 이 비계측 평균에 포함하지 않았다.

## 1. PC 고정 필드 탐색

GUI 진입점은 [SearchControls.svelte][pc-controls]와 [solverWorkspaceModel.ts][pc-ui]이다. 초안 값은 보존하고, 실제 실행 직전에 결과 제품에 맞게 활성 옵션을 정규화한다. 다음 표의 **기존**은 이번 커밋 이전부터 존재하던 구현, **이번**은 이번 커밋에서 바뀐 구현을 뜻한다.

| 옵션 범주 | 실제 실행 옵션·소유자 | 가지치기·재사용과 이번 변경 | 실행 검증과 경계 |
|---|---|---|---|
| 시작 보드, 목표 줄 수, 고정 큐·패턴 입력 | GUI가 `--board-mask`, `--height`, `--pieces`, `--queue` 또는 `--patterns`를 구성. Core의 [geometry_domain.rs][geometry]와 공급 상태가 기하 후보를 제한 | **실험으로 유지·기본값 미채택:** 최소 지지 수 피벗을 이길 수 없는 셀의 나머지 incidence 검사를 중단하는 cutoff를 구현했지만, 비계측 비교에서 개선 근거가 없어 제품은 전체 스캔으로 복귀. 명시 cutoff 경로는 피벗·동률 순서·뒤늦은 지지 수 0 검출을 보존 | Core 선택 회귀에 모든 미노 마스크, 빈틈·시작 필드·공급 변화가 포함. 기본값 복귀 후에도 명시 cutoff를 reference와 비교하는 검사를 유지. 큰 보드의 모든 형태를 열거한 검증은 아님 |
| 일반 해, Tiling, 전체 Path, 실패 큐 | 일반 해는 `pc --count unique`; 나머지는 `pc tiling`, `pc path`, `pc failed-queue`. [App 서비스][app]가 제품별 최종 결과를 소유 | **기존:** Tiling은 기하 배치, BuildUp은 실제 도달 가능성, Path는 완전한 경로의 의미를 유지. 공유 엔진의 `CountAll`에서는 같은 종류 미노의 서로 다른 공급 동작을 Boolean 커버리지처럼 합치지 않음 | P7 Core 25개 조합에 Unique/All 포함. WASM 일반 PC Path 통과. GUI에 독립적인 CountAll 선택기가 있다는 의미는 아님 |
| 최소 해 집합·필수 해 고정 | `pc minimals` / `pc pinned-minimals`; [pc_minimum_cover_result.rs][minimum]가 전체 후보 사전과 선택 포트폴리오를 소유. 문서·원본 집합 해시를 함께 전달 | **기존:** 전체 후보의 원래 큐 커버리지를 유지한 뒤 최소 포트폴리오를 선택. **이번 검증 보정:** 전체 원본 후보 수와 선택된 최소 집합의 크기를 서로 다른 계약으로 비교 | 최종 WASM 회귀에서 원본 후보 **4개**, 선택 집합 **1개**, 키·행 커버리지·전체 합집합·공개 포트폴리오의 일치 확인. 비교용 원본 실행에 `--solution-probabilities`를 명시. 임의의 개수 일치로 검사를 약화하지 않음 |
| 평균 점수, 고정 큐 최고 점수, 최고 점수 최소 집합 | `pc score`, `pc score-finder`, `pc score-minimals`; App 점수 제품과 Scoring 소유자. `score-finder`는 Jstris Ultra·T-spins·초기 B2B 0/1, 나머지 점수 제품은 선택한 프로필 사용 | **이번:** 초기 B2B 유효성 검사는 실제 점수 제품에서만 실행. 비활성 입력에 남은 NaN·음수 등이 일반 탐색을 막지 않음. 점수 제품의 평균·최댓값·최소 포트폴리오를 일반 존재성 집계로 치환하지 않음 | WASM 점수 3제품의 직렬/분산 공개 payload 일치, 전체 universe를 분모로 한 평균 회귀 통과. 점수 프로필 3종 × 모든 초기 B2B 값의 전체 조합은 미실행 |
| Hold, 공급 종료, Oracle / Visible 7 | Hold 사용 여부와 큐 지식이 Core 공급 automaton, [standard_bag_coverage.rs][supply], [PC 실행 정책][pc-policy]로 전달 | **이번:** 빈 Hold의 `StoreCurrent` 분기에서 같은 미노로 이어지는 여러 후속 간선에 공통인 공급 전이를 간선 반복문 밖에서 한 번 계산하여 재사용. 같은 종류 Hold의 Boolean 중복 제거는 **기존 구현**이다. 원래 큐 ordinal, Hold 방출, witness와 memo 재활용 계약 유지. Visible 7의 관측 전 의사결정 제약은 Oracle과 구분 | 공급 회귀 **5개** 통과. 별도 고정 큐·Hold 비교와 유한 공급 끝의 Hold 방출 검증도 통과. PC Visible 7 WASM 검사는 전역 직렬 policy finalizer로의 **라우팅** 검증이며 모든 정책 결과의 동치 증명은 아님. 최소 제품의 Visible 7은 명시적으로 지원 불가 처리 |
| 규칙·킥, 스핀 판정, B2B 보존 | GUI 규칙은 SRS+, SRS, SRS-X, Jstris 180. 6종 스핀 프로필과 B2B 조건이 [BuildUp][buildup], [extended BuildUp][extended], 공통 Scoring 판정으로 전달 | **기존:** compact 경로의 B2B 불가능 clear·죽은 suffix 제거. **이번:** extended 일반 PC에서도 `before_spin_evidence`가 확실히 거부하는 clear를 lock 도달성 계산 전에 제외. 스핀 증거가 필요한 경우에는 기존 정확 계산을 유지하고, child projection은 중복 계산하지 않음 | 작은 고정 입력에서 5 Core 규칙 × 6 스핀 프로필 × 6 fixture로 이전 늦은 필터와 비교. 거부 예제에서 도달성 상태 방문이 0이 됨을 확인. Core의 다섯 번째 NoKick 검사는 현재 GUI 규칙 선택기에 NoKick이 노출된다는 뜻이 아님 |
| 개별 해 확률 표시 | 일반 해·최소 해 집합에서 `--solution-probabilities`; Core 행 커버리지와 App/WASM 확률 보고가 소유 | 표시 여부가 해 집합이나 전체 큐 universe를 바꾸지 않아야 함. Skip 비교에는 개별 relation의 존재를 요구하지 않되, B2B 후처리 등 다른 소비자에게 필요한 relation을 임의로 제거하지 않음 | 아래 P7 **B2B × 개별 확률 2×2** 최종 App/WASM 비교 통과. Tiling·Path·실패 큐·점수 제품에서는 이 토글을 비활성화하고 실행 옵션에서도 제외 |

### GUI Path와 B2B Path 후처리는 구분해야 한다

GUI의 Path 제품은 B2B 보존 토글이 비활성화되고 실행 요청에서도 `preserveB2B=false`로 정규화된다. 이번 [App 서비스][app]와 [pc_path_result.rs][path-result] 변경은 **실제로 B2B 제약을 받은 정확한 WASM PC Path 요청**에서 eager replay를 뒤로 미뤄 중간 자료의 동시 생존을 줄인다. 따라서 이 변경을 “GUI Path에서 B2B를 켜면 적용되는 개선”이라고 설명하면 부정확하다. 일반 GUI Path는 별도 회귀로 확인했고, B2B Path는 공개 App API 하네스 10개 사례로 확인했다.

## 2. Build·Recovery·Setup 옵션

| 옵션 범주 | 실제 실행 옵션·소유자 | 기존 구조와 이번 개선 | 실행 검증과 경계 |
|---|---|---|---|
| Build 시작·목표 필드, 높이, 공급 길이, Hold | [buildProbabilityModel.ts][build-ui] → CLI/App Build 제품 → [build_probability.rs][build-core]. `sourcePieces`는 실제 평가할 공급 길이 | 목표 필드 기하와 공급을 별도로 제한하며 공급 길이는 Tiling에서도 의미가 있음. 큐 없는 최소 제품의 암시적 표준 bag 길이는 기존 규약에 따라 목표 미노 수와 Hold lookahead로 제한 | Desktop의 `desktop_build_source_pieces_changes_executed_universe_and_coverage`와 WASM의 유한 공급 EOF Hold 회귀가 실제 통과. P7 하나로 공급 변화 검증을 대신하지 않음 |
| Build 집계: buildability / tiling / spin | `all-solutions`에서만 선택 집계와 관련 세부 옵션이 활성. Tiling은 이동·스핀·B2B·Finesse 옵션을 비활성화 | **기존:** 존재성, 기하 배치, 스핀 실행의 의미를 분리. mirror pass는 중복 해와 원래 큐 커버리지를 정확히 합침. 목표와 무관하거나 완주 불가능한 BuildUp suffix는 Finesse 주석 계산 전에 제거 | WASM Tiling 직렬/분산 일치와 서로 다른 mirror pass 합집합 통과. B2B Build GUI argv의 직렬/분산 실행 통과. 모든 집계 × 규칙 × 스핀 조합은 미실행 |
| Build Finesse 입력 수, Oracle / Visible 7 / Both | GUI의 Finesse 지식 선택은 기본 Build 확률의 Oracle 의미와 별도. [build_probability.rs][build-core]가 비용 언어·큐별 최적 비용·witness를 소유 | **이번:** Oracle은 이미 구한 **각 큐별 최솟값**을 재사용해 불필요한 합성 언어 평가를 생략. 평균들의 최솟값으로 바꾸지 않으며 가중치와 동률 witness 유지. Visible 7/Both에는 관측 전에 함께 결정해야 하는 언어를 유지. 유지 자료·미래 할당·보고서의 동시 생존 메모리 계산 보강 | Core Oracle 비용·가중치·동률·고정 큐 witness, Visible 7의 joint language, report 메모리 1-byte 경계 회귀 통과. WASM 직렬/분산 Finesse 보고·witness 일치 통과 |
| Build replay·최소·점수 3제품·실패 큐, 필수 해 고정 | Build 결과 모드별 App 제품과 [buildProbabilityModel.ts][build-ui]가 소유. 실패 큐 표시 한도, 점수 프로필·초기 B2B, 최소 해 문서·원본 해시는 해당 제품에 전달 | **기존 동작 유지·검증:** `all-solutions` 외 제품은 buildability로 정규화하고 B2B·개별 확률·Finesse 등 비활성 옵션을 제외. 초기 B2B 검사는 기존의 점수 제품 한정 정책을 유지. **이번 Build 모델 변경**은 베타 prepass의 저장 초안 무시와 실행 비활성화이며, 위 PC GUI의 초기 B2B 검증 수정과 구분 | 큐 없는 Build minimum의 분산 terminal 실행 통과. GUI 투영·직렬화 검증 포함. 이 행의 모든 결과 모드를 모든 공급 길이로 실행한 것은 아님 |
| Recovery 중간 필드 수·각 단계 공급·결과 좌표계 | [recoveryStages.ts][recovery-stages], [recoveryBuildModel.ts][recovery-ui] → App [native_recovery_build_execution.rs][recovery-native] / WASM [recovery_build_projection.rs][recovery-wasm] | **이번:** `-`가 첫 감소 후 잠기는 조건을 실제 중간 필드 수 기준으로 수정. **기존 동작 유지·검증:** 삭제 시 빈 단계 우선 처리, 남은 mask·공급의 출처 보존, 단계 입력·결과 좌표계 변환 | GUI에서 5→4→3→2→1 감소, 비어 있는 단계·공급 보존 검증. WASM 정확 메모리 경계에서 실행 입력·예제가 유지됨을 확인. 모든 다단계 공급 조합 및 실제 브라우저 연속 클릭은 미실행 |
| Recovery 선행 설치 수 auto/수동, 미노 교환, Hold, B2B, 규칙·스핀, 최소·확률·PNG | 위 모델이 명시적 실행 옵션과 선택한 원본/필수 해를 전달. PNG·확률 표시는 결과 표현에도 사용됨 | `maxEarly=auto`는 실제 제품 기능이므로 유지. B2B는 해당 경계 탐색의 제약으로 전달하며 PC 점수 제품의 초기 B2B와 혼동하지 않음. 사용되는 표시 옵션은 제거하지 않음 | Build/Recovery 옵션 회귀 **26개** 통과. 다단계 × 교환 × Hold × 모든 규칙의 전체 Cartesian 검증은 아님 |
| Setup 우선순위, 잔여 미노·QB, 다음 cycle, 길이·최대 미노 수, 차용, 규칙·지식 | [setupFinderModel.ts][setup-ui] → `setup joint/build/pc` 및 세부 경로 요청 → [setup_partial_build.rs][setup-core] | **기존 개선 재사용:** 선택한 setup 상세와 맞지 않는 prefix를 움직임 탐색 전에 거르고, 승인된 pose가 나오기 전에는 placement identity를 저장하지 않음. 후속 continuation의 의미 유지. 이번 커밋에서 새로 만든 최적화로 집계하지 않음 | Core setup 회귀 **4개**가 35개 선택 회귀 안에서 통과. 모든 Setup 우선순위·QB·다음 cycle 조합과 native 2-worker 상세 경로 전체를 실행한 것은 아님 |

## 3. 삭제한 베타 버튼과 자동 의존성 처리의 차이

`미노 의존성 사전 분석(베타)`는 선택적인 사전 feasibility 분석을 켜는 UI였다. 버튼·관련 UI 문구를 삭제했으며, 옛 저장 초안의 `precomputeBuildDependencies=true`도 실행 정규화에서 무시한다. 일반 PC/Build 실행의 해당 경로는 `--no-build-dependency-dag`를 명시한다. **기존 정확 탐색 내부에서 자동 적용되는 후보 제약, BuildUp 가지치기, 공급 상태와 커버리지 계산은 계속 사용한다.** 이 작업에서 DAG 직접 풀이를 제품 실행으로 도입하지 않았다.

또한 [placement_dependency.rs][dependency-experiment]의 fused language·operation-mask 실험은 `test` / `local-search-ab`와 별도 opt-in으로 제한된다. 이 실험의 공급·커버리지 테스트가 통과했다는 사실을 일반 제품 빌드에 자동 활성화되었다는 주장으로 확대하지 않는다.

## 4. CPU, Auto, 가속기 및 결과 수명

| 범주 | 현재 동작과 이번 처리 | 경계 |
|---|---|---|
| GUI CPU 실행과 GPU 옵션 | PC/Build GUI는 CPU로 실행하고 GPU warmup과 관련 UI 실행 경로를 정리. 공통 [searchExecutionModel.ts][execution-ui]의 비GUI backend 타입·명시 호출 경로는 유지 | 공통 GPU/Hybrid/Auto 지원 전체를 삭제한 작업이 아님. 호출자의 의미가 있는 기능은 사용되지 않는 UI와 구분 |
| CPU Auto·전체 논리 프로세서 | [PC 실행 정책][pc-policy], [distributed_runtime.rs][distributed]가 실제 호스트 용량과 요청을 해석. GUI의 전체 CPU 토글, Setup의 `--auto-workers`, Recovery의 `maxEarly=auto`, Setup 길이 auto는 계속 사용 | native에서는 가상 host capability가 실제 CPU 용량을 넘길 수 없음. CountAll 회귀는 요청 11을 유지한 채 공개 정책으로 유효 worker 수를 계산하고, 분산 가능할 때 coordinator 사용을 검증. 이번에 자원 정책을 임의로 완화하지 않음 |
| tablebase·legal-board·conditioned-reachability·패턴 상한 | 공유 모델과 비GUI 호출에서 사용하는 옵션은 보존. 제품별 실행 투영, 규칙·목표 변경에 따른 tablebase 무효화, 명시적 비지원 처리 유지 | 현재 모든 항목이 SearchControls의 독립 토글로 노출되는 것은 아님. 모든 외부 가속기 프로필의 켜기/끄기 성능·정합성 행렬은 이번 범위 밖 |
| 공급·Finesse·Path 자료 수명 | [standard_bag_coverage.rs][supply]의 공유 불변 표와 worker memo를 구분하고, [build_probability.rs][build-core]의 비용 언어·보고서·부모 소유 자료가 함께 살아 있는 구간을 메모리 계산에 포함. App B2B Path는 중복 eager replay를 지연 | 메모리 개선은 Peak RSS만으로 설명하지 않으며, 구간별 유지 자료와 공존 관계를 별도 벤치 계측으로 확인. GUI 옵션 회귀의 통과가 특정 RSS 감소율을 입증하는 것은 아님 |
| 진행 상태와 협력적 GC | [gui_job_runner.rs][progress]에서 진행 상태를 최신 샘플 중심으로 제한하고 완료·종료 시 소유권을 정리. [clearra-node-gc.cjs][node-gc]와 [runtime.rs][runtime]가 관리 대상 Node의 압력 응답을 처리 | GC 완료는 일치하는 요청 ID·PID 응답으로 판단. Node의 GC 완료를 Rust/WASM heap 회수로 표현하지 않음 |

## 5. 최종 P7 옵션 비교 결과

검사: `p7_gui_b2b_probability_toggles_preserve_original_queue_coverage` ([lib_tests.rs][wasm-tests]). 입력은 4줄 `board-mask=0x3c0f03c0f`, 6개 설치, P7 공급, 빈 Hold 사용, SRS+, CPU 1 worker다. B2B 보존 시 T-spins를 사용한다.

| B2B 보존 | 개별 해 확률 | 정규화된 해 수 | 덮는 원래 큐 수 | 전체 큐 universe |
|---|---|---:|---:|---:|
| 끔 | Skip | 246 | 5,040 | 5,040 |
| 끔 | Include | 246 | 5,040 | 5,040 |
| 켬 | Skip | 4 | 724 | 5,040 |
| 켬 | Include | 4 | 724 | 5,040 |

네 실행 모두 최종 App 결과와 WASM 공개 보고를 비교했다. 각 B2B 상태에서 Skip/Include의 해 키·집합 해시·전체 커버리지 비트열·개수·확률이 정확히 같았다. B2B를 켠 해 키와 전체 비트열은 끈 결과의 부분집합이며, Include에서는 **각 해별 원래 큐 ordinal 비트열**도 대응하는 무제약 행의 부분집합이었다. 해별 행 합집합은 전체 커버리지와 일치했다. B2B로 제외된 큐 때문에 분모를 724로 재정규화하지 않고 5,040을 유지했다.

기존 Core P7 검사는 별개다. 5규칙 × 2 Hold × 2 Count의 20개 조합과 SRS+/Hold/CountAll에서 추가 스핀 5개, 합계 **25개 조합**에서 semantic pruning의 끄기/켜기를 비교했다. 이 25개는 모두 사용자 B2B 보존이 켜지고 개별 확률이 Include인 조건이다. 새 2×2 검사는 이 공백을 보완하지만 **25개 + 4개를 모든 옵션의 전체 Cartesian으로 표현하지 않는다.**

## 6. 실행 근거와 남은 검증 경계

| 최종 확인 항목 | 결과·근거 | 의미 |
|---|---|---|
| Core 선택 회귀 | [core-fixture-corrections.log][core-log]: 35 통과, P7 장시간 검사는 이 묶음에서 1 ignored | geometry cutoff, compact/extended 의미 보존, Finesse, 공급 실험, Setup, 계측 구분 포함 |
| 공급 전이 별도 회귀 | [core-supply-transition-final.log][supply-log]: 5 통과 | 공급 cursor·Hold, 원래 ordinal·witness, 공유 표·memo·재활용 확인 |
| 기존 P7 25조합 | [p7-option-matrix-final.log][p7-core-log]: 해당 ignored 테스트를 별도 명시 실행하여 통과 | 원래 큐 커버리지, 카운트, 완료된 제약 경로 보존 |
| WASM P7 2×2 | [stderr][p7-product-log]에 네 결과 출력, [stdout][p7-product-out]에서 해당 테스트 통과 확인 | 사용자 B2B 옵션과 개별 확률 표시의 실제 최종 제품 비교. 같은 배치의 minimum 실패는 아래 별도 최종 실행에서 해결 |
| WASM 기존 선택 회귀와 후속 수정 | [wasm-host-stack-tests.log][wasm-initial-log]에서 Build·mirror·Finesse·score·Visible 7 라우팅·Recovery 등 10개 통과. 이후 worker 정책과 일반 PC Path 통과 확인. minimum은 [wasm-minimum-source-relation-final.log][minimum-log]에서 최종 통과 | 초기 2개 실패를 숨기지 않고 원본/선택 집합 계약 및 실제 worker 정책에 맞게 검사 보정. P7와 같이 실행한 중간 로그에는 minimum 비교용 relation 누락 실패가 남아 있으므로 해당 배치 전체를 성공 로그라고 읽으면 안 됨 |
| App 공개 API | [app-public-validation-v3.log][app-log]: 고정 입력 5종 × TSpins/AllMiniPlus의 10사례 통과 | 실제 공개 경로의 B2B Path 결과·후처리 확인 |
| App 비공개 단위 테스트 | [app-test-typecheck.log][app-typecheck-log]: 관련 4개 타입 검사 통과, 실행은 미완료 | 전체 App 단위 테스트 바이너리 빌드가 SIGKILL된 제약 때문에 공개 API 실행과 구분. 4개를 실행 통과 수에 더하지 않음 |
| Desktop 실행·진행 상태 | [desktop-final-tests.log][desktop-log]: 10 통과 | 실제 Build 공급 길이 변화, 결과 문서 지연 생성, 작업 완료·취소·후속 실행, 최신 진행 샘플 및 terminal 경계 검증 |
| GUI 및 Build/Recovery | [gui-final-tests.log][gui-log]: 54 통과. [build-recovery-option-tests.log][build-ui-log]: 26 통과. [gui-final-typecheck.log][ui-types-log]: TypeScript 검사 통과 | 옵션 투영·명령 직렬화·중간 단계 감소·비활성 값 처리의 검증. 실제 브라우저 E2E 클릭 검증과 구분 |

반복 실행·부분집합 테스트 수를 하나의 총합으로 더하지 않았다. 아직 검증하지 않은 주요 영역은 모든 GUI 조합의 전수 실행, 임의의 다단계 Recovery 공급 변화, 모든 Visible 7 정책 결과, 모든 점수 프로필·초기 B2B 조합, 실제 브라우저 E2E다. 미실행 범위를 제품에서 옵션이 누락되었다는 의미로 해석해서도 안 되며, 코드 경로가 연결되었다는 사실만으로 실행 검증을 마쳤다고 표현해서도 안 된다.

[pc-controls]: ../../packages/clearra-ui/src/lib/workspace/SearchControls.svelte
[pc-ui]: ../../packages/clearra-ui/src/lib/workspace/solverWorkspaceModel.ts
[geometry]: ../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs
[app]: ../../crates/clearra-app/src/app_services.rs
[minimum]: ../../crates/clearra-app/src/pc_minimum_cover_result.rs
[supply]: ../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs
[pc-policy]: ../../crates/clearra-pc-graph/src/request/pc_execution_policy.rs
[buildup]: ../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs
[extended]: ../../crates/clearra-core-executor/src/backend/wasm_cpu/extended_buildup.rs
[path-result]: ../../crates/clearra-app/src/pc_path_result.rs
[build-ui]: ../../packages/clearra-ui/src/lib/workspace/buildProbabilityModel.ts
[build-core]: ../../crates/clearra-core-executor/src/backend/wasm_cpu/build_probability.rs
[recovery-stages]: ../../packages/clearra-ui/src/lib/workspace/recoveryStages.ts
[recovery-ui]: ../../packages/clearra-ui/src/lib/workspace/recoveryBuildModel.ts
[recovery-native]: ../../crates/clearra-app/src/native_recovery_build_execution.rs
[recovery-wasm]: ../../crates/clearra-wasm/src/wasm_command_runtime/recovery_build_projection.rs
[setup-ui]: ../../packages/clearra-ui/src/lib/workspace/setupFinderModel.ts
[setup-core]: ../../crates/clearra-core-executor/src/backend/wasm_cpu/setup_partial_build.rs
[dependency-experiment]: ../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_dependency.rs
[execution-ui]: ../../packages/clearra-ui/src/lib/workspace/searchExecutionModel.ts
[distributed]: ../../crates/clearra-wasm/src/distributed_runtime.rs
[progress]: ../../crates/clearra-gui-host/src/job/gui_job_runner.rs
[node-gc]: ../../scripts/runtime/clearra-node-gc.cjs
[runtime]: ../../tools/clearra-manage/src/runtime.rs
[wasm-tests]: ../../crates/clearra-wasm/src/lib_tests.rs
[core-log]: ../../_local/artifacts/gui-option-optimization-20261124/core-fixture-corrections.log
[supply-log]: ../../_local/artifacts/gui-option-optimization-20261124/core-supply-transition-final.log
[p7-core-log]: ../../_local/artifacts/gui-option-optimization-20261124/p7-option-matrix-final.log
[p7-product-log]: ../../_local/artifacts/gui-option-optimization-20261124/wasm-final-targeted-v2.log
[p7-product-out]: ../../_local/artifacts/gui-option-optimization-20261124/wasm-final-targeted-v2.out
[wasm-initial-log]: ../../_local/artifacts/gui-option-optimization-20261124/wasm-host-stack-tests.log
[minimum-log]: ../../_local/artifacts/gui-option-optimization-20261124/wasm-minimum-source-relation-final.log
[app-log]: ../../_local/artifacts/gui-option-optimization-20261124/app-public-validation-v3.log
[app-typecheck-log]: ../../_local/artifacts/gui-option-optimization-20261124/app-test-typecheck.log
[desktop-log]: ../../_local/artifacts/gui-option-optimization-20261124/desktop-final-tests.log
[gui-log]: ../../_local/artifacts/gui-option-optimization-20261124/gui-final-tests.log
[build-ui-log]: ../../_local/artifacts/gui-option-optimization-20261124/build-recovery-option-tests.log
[ui-types-log]: ../../_local/artifacts/gui-option-optimization-20261124/gui-final-typecheck.log
