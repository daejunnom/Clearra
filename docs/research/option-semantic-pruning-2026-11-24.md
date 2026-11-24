# 옵션 의미를 반영한 의존성 경로 가지치기

작성일: 2026-11-24 (Asia/Seoul)

> 이 문서는 `b75a1cc708389e50cd069ba5cbe647d351838664` 소스 초안과 실행 owner가
> 해결되기 전의 상태를 보존한 기록이다. 이후 사용자의 고아 작업 취소 허용에 따라
> 작업을 재개했다. 실제 GUI/제품 회귀, P7/P7P4 실행, 구간별 메모리, 최종 적용
> 결정은 [후속 검증 보고서](gui-option-performance-2026-11-24.md)를 참고한다.

## 범위와 현재 상태

이번 변경은 요청의 의미를 기존 후보별 BuildUp 의존성 경로와 그 소비자에 적용한다.
후보 family를 직접 푸는 새 DAG solver는 추가하지 않는다. Full 6L은 완전 보류하며,
CI 파일 변경·실행·조회, 원격 push, main 승격은 수행하지 않았다.

소스 변경과 독립적인 읽기 검토, 지정 파일의 Rust 1.98.1 rustfmt,
`git diff --check`까지 수행했다. **컴파일, 단위 테스트, P7 대조 실행, 성능 측정은
아직 수행하지 않았다.** 아래 테스트 숫자는 작성된 사례 수이며 통과 실적이 아니다.

### 정확한 기준 소스

- GitHub에서 새로 확보한 `daejunnom/Clearra`의 `origin/main`:
  `8fa2dfc6ee195206f50a241d9888125ffaa67dbe`.
- 작업 브랜치: `codex/option-semantic-pruning-20261124`.
- 체크아웃: `/workspace/scratch/30e3cf309daa/Clearra`.

이전 `/workspace/scratch/0de33b7579b7/Clearra` 체크아웃은 현재 공간에서 찾을 수 없었다.
이전 로컬 HEAD `1250a74c50716415b1a086e4d2e4bb159c423dd4`는 GitHub에서도
조회되지 않았다. 이전 미게시 draft `896bb3f846a1132f8114699bdf0bf6fe6b50d852`의
정확한 패치 보존본도 찾지 못했다. 따라서 이번 변경은 그 커밋들의 복구본이 아니며,
이전의 77개 테스트 통과나 P7P4 측정값을 이번 소스의 검증 근거로 사용하지 않는다.

### 실행을 시작하지 않은 이유

이전 managed release build의 실행 ID `1795475629633014911-5`가 현재도
`memory-intensive-0.lock`의 owner로 기록되어 있고, 그 실행의 종료 receipt를
찾지 못했다. 이전 실행 세션 handle의 소실은 전체 descendant 종료를 증명하지 않는다.

저장소 `AGENTS.md`의 memory/process-tree supervision 계약과 기존 세션의
실행 제약에 따라 슬롯을 삭제하거나 다른 profile·상태 루트로 우회하지 않았다.
검증 재개에는 해당 실행의 정상 종료/전체 프로세스 트리 종료를 확인하는
receipt 또는 원래 실행 controller의 종료 확인이 먼저 필요하다.

## 기획과 실행 경로의 연결

기획 근거는 [Rust core/relation roadmap §5.5](rust-core-and-relation-roadmap-2026-11-20.md),
[Setup Finder](../setup-search.md), [dependency boundary](../dependency-boundary.md)다.

현재 확보한 원격 소스에서 `SymbolicPrefixOrders`는 순수 Boolean 실험 범위이고,
`RealizationFeasibilityWorkspace`의 완전 prepass는 기본 비활성이다. 이곳에 조건만
추가해서는 실제 제품 계산이 줄어들지 않는다. 이번 변경은 실제 사용되는
`BuildOrderGraph::build_pass`, Finesse annotation, Setup partial-build 생성에 연결한다.
이전 미게시 symbolic 경로 일반화 전체가 포함되었다고 주장하지 않는다.

| 요청 의미 | 적용 위치 | 생략할 작업 | 유지할 근거 |
|---|---|---|---|
| B2B-save, T-only profile의 non-T 일반 삭제 | 조건부 scoring 그래프의 operation 확장 전 | 해당 operation의 모든 pose 이동 검사와 후속 간선/노드 | 같은 parent/child projection에 대한 정확한 삭제 사건 |
| B2B-save, spin-capable transition | 기존 정확한 spin evidence 소비 | 거짓으로 단정해 생략하지 않음 | 실제 마지막 회전·ordered kick·immobile/corner evidence |
| B2B-only Finesse의 0줄·4줄·PC | Finesse annotation | 불필요한 scoring lock evidence 조회 | 기존 clear 예외 및 별도 Finesse movement/cost 검사 |
| 완료 경로 없는 Finesse source/child | 기존 accepting-suffix liveness 소비 | evidence, target group, movement BFS | 실제 완료 경로를 포함하는 원래 그래프의 suffix 증명 |
| 이미 거절된 Finesse target | 공용 target-group helper | target 복사와 BFS; 전체 target이 거절되면 query 자체 | 출력 cost/evidence 슬롯을 None으로 먼저 정리 |
| CountAll | 모든 위 경로 | 실패 경로만 제거 | 살아 있는 concrete pose, 원본 공급/Hold multiplicity 유지 |
| 선택된 Setup detail | 줄 삭제 projection 직후, 이동 검사 전 | 목표 prefix가 될 수 없는 배치와 후속 상태 | 정확한 원본 행 삭제 mask, 목표 board, 선택 depth |
| 모든 Setup 모드 | 첫 허용 pose 직후 | 실패한 row의 placement-set 저장/인덱싱 | 허용 pose들 사이에서 동일한 canonical row set 공유 |

## B2B-save 정책의 단일 의미 권위

`clearra-scoring::BackToBackPreservationPolicy`에 할당 없는 사전 판정을 추가했다.

```rust
pub enum BeforeSpinEvidenceDecision {
    Preserves,
    Rejects,
    RequiresSpinEvidence,
}
```

판정 입력은 piece, 해당 lock의 정확한 삭제 줄 수, 해당 lock 직후의 PC 여부다.
미래 목표가 PC라는 사실을 현재 lock의 PC로 취급하지 않는다.

| 조건 | 결정 |
|---|---|
| 0줄, 4줄, 또는 해당 lock의 PC | Preserves |
| 활성 spin 인식이 없는 piece의 일반 삭제 | Rejects |
| Simple T profile의 T | Preserves |
| T corner/plus 또는 허용된 non-T immobile spin | RequiresSpinEvidence |

기존 `allows()`도 같은 분류를 소비하며, 마지막 경우만 기존 `SpinDetector`를
호출한다. 기존 save 계약은 recognition을 사용하므로 award mapping을 새 거절
조건으로 넣지 않았다. 2줄 PC는 save에서 허용되면서 숫자 B2B chain은 끊길 수 있다.
두 개념을 혼동하지 않는다.

executor는 기존 `clearra_postprocess::BackToBackEdgePolicy`를 통해 이 계약을
소비한다. 새로운 `core-executor -> scoring` 직접 runtime 의존성을 추가하지 않았다.

### Operation 전체를 제거할 수 있는 이유

하나의 후보에서 실행한 operation 집합 P는 `CandidateProjection`의 물리 board와
원본 행 삭제 집합 D(P)를 결정한다. 다음 operation의 모든 유효 realization은
같은 child projection과 일치해야 한다. 따라서 새 삭제 줄 수는
`popcount(D(child) & !D(parent))`이고, 해당 lock의 PC 여부 역시 child board로 결정된다.

이 정확한 clear 사실과 piece에 대해 정책이 Rejects이면, 그 operation의 모든
유효 pose가 요청 조건에 실패한다. 이때 이동 검사 전에 operation을 제외할 수 있다.
spin-capable piece에는 이 단계의 강한 부정이 없으므로 불필요한 child projection
선행 계산도 피한다. 정책 거절은 공유 physical reachability/feasibility cache에
저장하지 않는다.

일반 PC/Build의 원래 coverage와 denominator를 만드는 graph에는 이 정책을
적용하지 않는다. 조건부 scoring evidence를 재구성하는 호출만 명시적으로 전달한다.
완전한 정책 거절은 identity와 비수락 root 한 개를 가진 **완전한 빈 graph**로 남긴다.
`None`으로 반환하면 호출자가 batch 불완전으로 판단하므로 구분이 필요하다.

## Finesse: 동률 선택을 유지하는 생략 지점

검토 중 geometry graph 생성 단계의 선행 제거가 부적절한 경계를 발견했다.
Finesse의 `transition_order`는 기존 edge ordinal이며, 여러 후보의 언어를 합칠 때
`CostedGeometryLanguage::union`이 action별 최소 ordinal을 사용한다. 후보별로 삭제된
간선 수가 다르면 비용이 같은 대표 경로의 우선순위가 달라질 수 있다.

최종 변경은 Finesse의 원래 geometry graph와 ordinal 공간을 유지한다. 그 이후
annotation 단계에서 dead suffix와 정책상 불가능한 target을 제거하고, 비용과
증거 계산을 생략한다. 압축된 간선에도 원래 index를 `transition_order`로 남긴다.

spin coverage가 함께 요청되면 0줄·4줄·PC라도 필요한 lock evidence를 유지한다.
Simple T의 사전 Preserves 판정도 기존 terminal-label 요구를 해제하지 않는다.
Boolean 조건 통과, 물리 이동 가능, 최소 입력 비용, 스핀/재생 증거는 서로 다른 계약이다.

## Setup: 선택 prefix와 이후 continuation 구분

선택된 detail의 정확한 board, 원본 행 삭제 mask, depth를 사용한다.

1. 현재 삭제 mask에 목표에 없는 원본 행이 포함되면 제거한다. 삭제는 복구되지 않는다.
2. 목표 depth에서는 board와 삭제 mask가 모두 정확히 일치해야 한다.
3. 현재 삭제 mask와 목표 삭제 mask가 이미 같을 때만 현재 board의 목표-subset
   검사를 적용한다. 동일한 compacted 좌표계이며, 목표 prefix 안에서 남은 삭제가
   없어 불필요한 셀을 제거하거나 옮길 수 없기 때문이다.
4. 아직 목표 삭제가 남아 있으면 단순 board-subset 검사를 하지 않는다. 이후 삭제가
   셀을 없애거나 아래로 이동시킬 수 있다.
5. 선택한 depth 이후의 PC continuation에는 이 prefix 제한을 적용하지 않는다.

선택 detail 경로에서는 lock/clear projection을 한 번 앞당겨 계산해 이동 검사와
상태 저장을 생략하며, 허용된 배치는 그 계산 결과를 재사용한다. 일반 Setup 및
선택 depth 이후 경로는 기존의 movement-before-projection 순서를 유지한다.

별도로 모든 Setup 모드에서 placement-set interning을 첫 허용 pose까지 지연한다.
전체 검색 동안 retained되는 placement-set 값과 depth별 보조 index에 실패 row를
넣지 않는다. 내부 ID 값은 달라질 수 있으나 공개 identity의 canonical row set과
허용 pose multiplicity는 유지한다.

## 메모리 소유권과 수명

| 구간 | 기존 owner 중복/불필요 자료 | 변경 후 수명 |
|---|---|---|
| B2B 사전 판정 | 불가능한 operation에서 만들어질 graph/evidence | inline policy만 보유; 실패 operation의 저장 이전에 종료 |
| Finesse target 준비 | dead source/child 및 rejected target의 group entry/Vec | 증명된 실패 target을 저장하지 않음 |
| Finesse group 실행 | 모두 거절된 target도 query/ordered-table clone/BFS 생성 | stale output을 지우고 query를 생략 |
| Finesse 간선 압축 | old operation edges, old piece edges와 별도 new edge 배열들이 동시 생존 | 두 기존 BuildEdge 버퍼를 재사용하며 안정적으로 앞으로 압축 |
| Finesse 출력 | 이전 전체 간선 수를 기준으로 prepared-edge 용량 확보 | 허용 cost와 live child를 가진 실제 생존 간선 수만큼 확보 |
| Finesse kick owner | annotation 진입 시 추가 outer clone | 기존 canonical owner를 borrow; 각 query의 필요한 소유권은 유지 |
| Setup identity | 허용 pose 없는 row도 영속 placement-set 값 생성 | 첫 허용 pose에서 생성, 나머지 pose가 공유 |

`PreparedFinesseNode::source_board`는 inline 값이다. dead node에서 None으로 두는
것은 생성/조회 작업을 줄이지만 node 자체의 바이트 수는 줄이지 않는다.
graph 버퍼 capacity는 다음 후보에 재사용한다. 따라서 배열 중복과 불필요 자료를
줄였다는 구조적 설명을 Peak RSS 감소나 실행시간 개선의 실측치로 바꾸어 말하지 않는다.

## 규칙·킥의 적용 경계

이번 가지치기는 piece/clear/profile의 요청 의미에만 의존하며, 살아 있는 transition은
기존 ordered-kick 물리 검사와 evidence 경로를 소비한다. 테스트에는 SRS, SRS+,
SRS-X, Jstris180, NoKick을 포함했다.

annotation 하나만 verified/imported table로 바꾸면 Finesse material과 메모리 상한의
builtin owner와 불일치할 수 있다. 사용하지 않는 table entry만 커진 프로필에서도
전체 table clone의 메모리 비용이 달라진다. 따라서 최종 변경은 기존 admission과
canonical table owner를 유지하고 outer clone만 제거했다. 이번 패치가 imported
profile의 제품 admission/소유권 전체를 일반화한 것은 아니다.

숫자 score prefix가 낮다는 이유만으로 경로를 제거하는 bound도 추가하지 않았다.
미래 점수·combo·spin·PC bonus를 포함하는 정확한 상한이 없는 경우에는 거절 근거가 없다.

## 검증 소스와 아직 필요한 실행

새 테스트는 다음 파일에 있다.

- `crates/clearra-scoring/src/b2b_preservation_tests.rs`: 기존 predicate와 전체
  profile/piece/clear 예외 및 구체적인 lock evidence 조합을 대조한다.
- `crates/clearra-core-executor/src/backend/wasm_cpu/buildup/execution_pruning_tests.rs`:
  완성된 기존 경로에 최종 B2B predicate를 적용한 multiset과 새 경로를 비교한다.
  빈 graph의 완전성, 불필요 query 생략, 원래 간선 순서와 버퍼 재사용, Build/Finesse
  결과와 witness를 확인하도록 구성했다. Scoring 경로 key는 전체 lock/corner evidence를
  포함한다. 여러 source node의 원래 ordinal `[1, 2, 4, 6]`과 서로 다른 입력 비용의
  대응을 확인하며, 반복 O 공급과 여러 tiling을 가진 IIOOO의 최종 Finesse report/
  representative witness도 비교한다.
- `setup_partial_build.rs`의 테스트: 원본 행 좌표·앞으로의 삭제·continuation 반례,
  작은 전수 모델, identity 저장 지연, 실제 movement query 생략을 다룬다.

P7 matrix는 명시적으로 실행하는 ignored 테스트다. 실제 Standard7Bag 7 draws의
5,040 원본, 보드 `0x3c0f03c0f`, 6 locks를 사용한다. 5 rules × Hold on/off ×
CountUnique/CountAll의 20 cases와 추가 spin profile 5 cases를 비교한다.
identity/key, global/per-identity coverage, 원본 pattern/cursor/Hold, CountAll,
probability/completeness, 조건을 만족하는 완료 경로와 witness를 비교한다.
이 matrix의 두 arm은 새 semantic-pruning의 off/on이다. 기존
`precompute_build_dependencies`는 기본값이며 그 플래그 자체의 on/off matrix까지
포함한 것으로 해석하지 않는다. 테스트는 TLS 대조를 위해 양쪽 모두 1 worker이며 성능 비교가 아니다.
제품 Auto/all-logical-CPU 정책은 변경하지 않았다.

읽기 검토 과정에서 Finesse ordinal의 cross-candidate 의미와 kick payload owner의
불일치 가능성을 발견하고 위와 같이 적용 경계를 수정했다. 이 검토는 컴파일과 실제
제품 대조를 대신하지 않는다. 실행 owner 정리가 확인되면 관리 경로로 다음 순서를
수행해야 한다.

1. scoring의 새 정책 테스트, executor의 작은 semantic-pruning 및 Setup 테스트.
2. 명시적인 P7 ignored matrix와 실제 조건부 App 결과 대조.
3. 기본/병렬 feature 및 해당 host caller의 typecheck.
4. 동일 source/config/worker 조건의 구간별 시간과 실제 메모리 소유량 측정.

실행 기록이 없으므로 speedup 백분율, RSS 감소량, P7 통과 또는 전체 옵션/제품
완성 주장은 하지 않는다. 원격 미게시 변경과 검증 공백을 분명히 남긴 소스 개선안이다.
