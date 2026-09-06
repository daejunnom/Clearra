# Exact minimum continuation 연결 감사 — 2026-09-06

## 상태와 범위

**이 문서는 구현 완료 보고서가 아니다.** 현재 dirty worktree의 Core/App/GUI 명령 연결을 읽기 전용으로 대조한 결과와, 아직 연결되지 않은 경로를 공통 cooperative continuation으로 연결하기 위한 최소 구현 경계를 기록한다. 아래의 기존 함수명과 관찰 위치는 현재 소스 근거이며, 제안하는 새 타입/함수명은 구현안이다. 현재 진행 중인 WASM 빌드의 소스는 이 조사로 변경하지 않았다.

이번 local coordinator + one-level idle assistance가 실제 연결되는 것은 **WASM cooperative `pc.minimals`의 최소값 증명과 첫 canonical 집합 선택**이다. 다른 제품이 같은 Exact Minimum Cover 엔진이나 같은 portfolio 결과 타입을 사용한다는 사실만으로, 그 제품에 멀티워커 scheduler가 연결되었다고 판단하면 안 된다. CLI의 native 동기 실행과 native 성능 predictor도 구별한다.

## 현재 연결 행렬

| 제품/입구 | 실제 결과 준비 경로 | 공유 exact solver 사용 | 새 local coordinator / idle assistance 도달 |
| --- | --- | --- | --- |
| PC Minimum Solutions, opening/scenario, WASM | `PcMinimals` → `PcMinimalsFinalize` → `PcMinimumCoverProductPreparation` → `CoveragePortfolioAlternativeSetPreparation` | 예 | **예** |
| GUI Build Minimum Solutions | GUI가 `clearra build cover --objective min-cover` 생성 → `validate_build_coverage_portfolio_v2_result` → `new_canonical` | 예 | **아니오** |
| PC `score-minimals`, opening/scenario | score postprocess → `ValidatedPcScorePortfolioExecutionEvidence::validate` → `validate_pc_score_portfolio_v2_result` → `new_canonical` | 예 | **아니오** |
| GUI Build Highest Score Minimum Set | `BuildProbabilityAppCommand::response_from_materialized_result` → `build_highest_score_minimum_payload` → `new_canonical` | 예 | **아니오** |
| typed Build colored target cover / score | `validate_build_colored_portfolio_v1_result` / `validate_build_colored_score_v1_result` → `new_canonical` | 예 | **아니오** |
| Build evaluate minimals / score | `validate_build_supplied_minimum_cover_v1_result` / `validate_build_supplied_score_v1_result` → `new_canonical` | 예 | **아니오** |
| Spin-structure coverage portfolio | `project_spin_structure_coverage` → `new_canonical` | 예 | **아니오** |
| PC minimals의 native 동기 product validation | `ProductCapabilityResult::validate_with_optional_pc_replay_source` → `prepare_pc_minimum_cover(...).complete()` | 예 | **동기 driver이며 새 WASM scheduler가 아님** |
| 최초 결과 이후 portfolio 페이지/대안 탐색 | 이미 만들어진 `CoveragePortfolioAlternativeSet`의 페이지 owner | 예 | 위 최초 결과 연결만으로 후속 페이지까지 병렬화되었다고 볼 수 없음; 별도 driver 확인 필요 |

이는 **최소 집합 선택 단계**에 대한 표다. Geometry, Build 검증, score 실행 자체가 병렬인지 여부와 혼동하지 않는다. score가 없는 전체 해법/확률 집계에 최소집합 solver를 억지로 적용하지 않는다.

### 핵심 코드 근거

- `packages/clearra-ui/src/lib/workspace/buildProbabilityModel.ts:226,274`: Build의 `minimum-solutions`는 PC 명령이 아니라 `build cover --objective min-cover`로 내려간다.
- `crates/clearra-app/src/cooperative_execution.rs:2938,2971`: `PcMinimalsFinalize` 진입은 `ProductCapabilityContract::PcMinimals`와 성공한 response에 한정된다.
- 같은 파일 `1705–1828`: enable, memory envelope, query, task, receipt, idle assist, redundancy 메서드가 모두 `PcMinimalsFinalize`만 위임한다.
- `crates/clearra-app/src/product_capability_result.rs:1313`: `prepare_pc_minimum_cover`는 `PcMinimals`만 받아들이며 opening/Pc 또는 scenario/Scenario render 계약을 검증한다. Build/Score response를 이름만 바꾸어 이 wrapper에 넣을 수 없다.
- `crates/clearra-app/src/portfolio_alternative_store.rs:491,530`: 공통 `CoveragePortfolioAlternativeSetPreparation`은 존재하고 proof/canonical 양쪽 `enable_parallel`을 지원한다.
- 같은 파일 `886`: `CoveragePortfolioAlternativeSet::new_canonical`은 preparation을 만든 뒤 **`enable_parallel` 없이 `advance(u64::MAX, false)`를 완료까지 호출**한다. 생성자에 parallel만 켜면 외부 task driver가 없으므로 해결되지 않는다.
- `crates/clearra-app/src/cooperative_execution.rs:2762` → `app_services.rs:1346,1386` → `pc_score_minimum_cover_result.rs:335,561`: PC score-minimals의 exact portfolio는 현재 score postprocess 호출 안에서 동기 완결된다.
- `crates/clearra-app/src/build_probability_product_result.rs:537,661`: GUI Build highest-score minimum 결과의 동기 portfolio 생성.
- `crates/clearra-app/src/build_solution_probability_result/build_v2_result.rs:195,292`: Build cover/min-cover 및 max-probability-minimum의 동기 portfolio 생성.
- `build_v2_colored_result.rs:641,671,699,778`, `build_v2_supplied_result.rs:669,708,842,1001`: colored/supplied cover와 score 제품의 같은 경계.
- `crates/clearra-app/src/spin_structure_coverage_result.rs:24,93`: spin-structure coverage의 같은 동기 생성자 사용.

현재의 PC 전용 memory envelope는 다른 제품의 병렬 실행을 금지하는 제품 정책이 아니라, **검증하고 소유량을 셀 수 있는 continuation이 아직 PC 전용이라는 구현 경계**다. 허용 variant를 늘리는 것만으로 이를 해제하면 안 된다.

## 최소 공통 seam: 검증된 입력 / exact 작업 / 제품 sealing 분리

새 제안 타입의 이름은 예시다. 기존 CLI 계약, 결과 스키마, 확률·점수 의미를 변경하는 개편이 아니다.

1. **제품별 검증된 입력 준비**

   각 기존 validator에서 `new_canonical` 이전까지의 검증과 행렬 구성을 분리한다. 결과는 `PortfolioAlternativeSetIdentity`, canonical candidate keys, required pattern set, coverage rows, 그리고 최종 report를 seal하기 위해 필요한 제품별 불변 context를 소유한다. 검증되지 않은 response나 formatter DTO가 이 입력을 만들 수 없어야 한다.

2. **공통 cooperative exact 작업**

   예: `CooperativeCoveragePortfolioWork`가 `CoveragePortfolioAlternativeSetPreparation`을 소유한다. 최소값 증명과 canonical 선택, 제한된 work quantum, cancel, parallel enable/query/task/receipt, idle assist/redundancy, retained-capacity projection을 이 한 owner에 위임한다. App의 바깥 state는 PC/Build/Score라는 문자열이 아니라 실제 pending cover-work 소유 여부를 통해 transport를 제공한다.

3. **제품별 완료 sealing**

   공통 작업이 반환하는 `CoveragePortfolioAlternativeSet`을 제품별 context와 결합한다. PC minimum, PC score portfolio, Build coverage, Build score, supplied/colored/spin 제품의 기존 completeness·identity·public candidate map 검증을 유지한다. exact 작업을 다시 수행하거나 Geometry/score derivation을 다시 실행하지 않는다.

4. **host별 driver**

   WASM은 existing distributed completion과 ready-worker pool을 재사용한다. 동기 실행은 같은 준비/완료 경계 위의 blocking driver로 유지할 수 있다. CLI native에 실제 병렬 driver를 연결하려면 별도로 명시적으로 구현하고 검사한다. GUI 전용으로 proof authority를 복제하거나 Discord 명령을 Core 내부 모델로 삼지 않는다.

### 기존 함수별 분할 책임

| 현재 owner/함수 | 유지할 검증 책임 | 분리할 exact 작업 / 완료 책임 |
| --- | --- | --- |
| `PcMinimumCoverV2Preparation`, `PcMinimumCoverProductPreparation` | PC opening/scenario binding, complete candidate source, resource evidence | 이미 있는 cooperative 패턴을 공통 work owner로 위임; PC report sealing 유지 |
| `validate_pc_score_portfolio_v2_result` | summary와 derivation winner의 fieldwise parity, 완전한 pattern universe, canonical candidate identity, score-only eligibility | `new_canonical` 이전에 준비 결과 반환; 완료 후 selected score candidate IDs, canonical representative, completeness 및 evidence sealing |
| `ValidatedPcScorePortfolioExecutionEvidence::validate` / score App service | 실행 source와 score summary의 provenance, derivation binding | pending 동안 validated score evidence/context 유지; exact 완료 전 evidence를 완성된 것으로 노출하지 않음 |
| `validate_build_coverage_portfolio_v2_result` | Build capability/options, complete producer count/probability/keys, coverage union, query/hash binding | 공통 작업에 candidate rows 전달; 완료 후 union probability와 canonical portfolio report 구성 |
| `build_highest_score_minimum_payload` | Build typed score derivation, eligible rows, public candidate ID map, product build identity | 공통 작업 완료 후 기존 payload 및 page-source owner 구성 |
| colored/supplied Build validators | target/supplied source identity, replay coverage, score winner eligibility, query options | 기존 immutable context를 보관해 완료 후 각 스키마로 seal; 서로 다른 source identity를 합치지 않음 |
| `project_spin_structure_coverage` | spin candidate identity와 coverage row count/order | 공통 작업 완료 후 기존 portfolio payload/owner 작성 |
| `CoveragePortfolioAlternativeSet::new_canonical` | 동기 caller의 호환 진입점 | 새 prepared work의 blocking adapter로 둘 수 있으나, parallel enable 여부만 바꾼 busy loop는 금지 |

## 의미와 자원 소유 경계

- PC/Build score-minimals의 행은 **각 pattern에서 최고 score를 달성하는 후보의 eligibility**다. 일반 minimum의 coverage 행으로 대체하면 안 된다. attack은 informational이며 tie나 coverage 자격에 혼합하지 않는다.
- Build의 성공 확률 union과 minimum portfolio 개수는 다른 값이다. 최소값/첫 canonical 증명이 완료되기 전에 모든 대안의 개수를 안다고 보고하거나 성공 확률의 의미를 바꾸지 않는다.
- 원본 candidate ID와 canonical dense portfolio ID의 mapping 및 set identity를 보존한다. 부모와 자식의 exact task는 동일한 원본행·query domain을 사용한다.
- query epoch는 matrix SHA-256 + generation + query ID를 전부 포함한다. proof/canonical 단계가 숫자 counter를 재사용할 수 있으므로 counter만 비교하지 않는다.
- App 공통 envelope에는 response, validated context, preparation, retained query/frontier, 완료 대기 report와 관련한 모든 실제 heap owner를 포함해야 한다. Build/Score context의 알 수 없는 크기를 `0`으로 취급하지 않는다.
- idle assist의 registry 교체 peak, active local shard, retry task, receipt decode 및 Found 결과 공존 peak도 기존 host admission에 계속 포함한다. 자원 부족은 정확한 원본 작업을 보존하는 scheduling decline이어야 하며 `Cancelled`를 `None` 증명으로 바꾸면 안 된다.
- score/replay source를 그대로 두고 exact preparation을 비동기로 만드는 과정에서 derivation/result를 불필요하게 clone하지 않는다. 움직인 owner와 공유 Arc의 집계 관례는 projection 계약에 명시한다.
- 각 제품의 GUI render/copy는 기존 page owner를 사용한다. 계산 중 CTK를 생성하거나 전체 alternative 집합을 먼저 열거하는 방식으로 바꾸지 않는다.

## 검증 포인트

### 연결 및 제품 parity

- 행렬의 각 applicable 제품에 최소 두 개 이상의 비자명한 후보가 있는 fixture를 넣는다. DTO 옵션 노출이나 `enable_parallel` 호출 횟수만이 아니라 **query 발행 → 둘 이상의 task 실행 → receipt 수락 → 완료**를 검사한다.
- PC opening/scenario, Build cover, PC score-minimals, Build highest-score minimum, colored/supplied cover/score 각각에서 direct/cooperative/distributed 결과 계약, canonical 첫 집합, public candidate map, 확률/score evidence를 비교한다.
- score-only tie에 attack이 다른 후보를 넣어 membership/canonical ordering이 attack으로 변하지 않는지 검사한다.
- empty-success source, 단일 후보, 중복 coverage 행, public ID가 dense ID와 다른 경우도 유지한다. 작은 경우 멀티워커가 불필요하게 강제되었다고 테스트를 만들지 않는다.

### exact task 및 lifecycle

- 증명 단계와 canonical 단계의 matrix identity를 바꾸되 숫자 query counter가 같은 경우 stale key를 거절한다.
- 원본 negative와 모든 child negative의 두 closure 경로, 누락 child, 활성 child cancellation, 이미 닫힌 원본의 늦은 첫 receipt, duplicate, replay-valid contradictory Found를 검사한다.
- 다음 query/최종 response가 실제 발행된 작업의 drain보다 먼저 완료되지 않게 한다. query publication 사이에 UI worker 표시나 task latch가 이전 job으로 새지 않아야 한다.
- work budget을 소진해도 Pending으로 복귀하며, user cancellation이 원격/로컬 작업과 pending product context를 함께 정리하는지 확인한다.

### memory와 성능

- 각 제품 owner에서 capacity padding, 공유 Arc, 아직 seal되지 않은 score/replay context를 포함한 실제 retained projection을 테스트한다. peak-minus-one decline과 충분한 admission의 경계를 검사한다.
- selected/coverage 준비, 최소값 proof, canonical 선택, transport gap, 최장 shard 시간을 따로 측정한다. 동일 실행환경·동일 source의 assist 0/1만 비교한다.
- 현재 native probe는 GUI 성능의 합격 근거가 아니다. WASM 초기화·복제·u128 연산·브라우저 scheduling 비용을 포함한 별도 GUI 계측이 필요하다.
- 새 경로를 추가한 뒤 해당 제품의 큰 사례를 따로 측정한다. PC fixture에서의 속도 개선을 Build/Score/Spin의 개선으로 일반화하지 않는다.

## 현재 남은 일

- 위 공통 continuation 및 각 제품 adapter 연결은 **미구현**이다.
- 현재 빌드 중인 WASM의 PC minimum/Build 성능 검증과 native release predictor A/B는 별도 진행 중이며, 이 문서가 성능 조건 충족이나 배포 Go를 뜻하지 않는다.
- PC minimum의 새로운 idle assist는 기존 원본 cursor를 유지한 **보조 complete-cube race**다. private DFS의 미탐색 continuation을 실제로 이전하는 work donation을 구현했다고 설명하지 않는다.
