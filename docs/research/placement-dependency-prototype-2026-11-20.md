# 배치 의존성 조건 그래프 초안

작성일: 2026-11-20 KST. 기준 main: `fb795810535e6b85dcb201c0f5c5ea8d6a3fad8d`.

## 목적과 구현 범위

미노의 구체적 배치 사이에서 도달성에 영향을 주지 않는 조건을 증명하고,
동일한 조건을 만족하는 다른 보드에서 기존 판정을 재사용한다. 같은 종류의
미노, 다른 공급 경로, 다른 삭제 행 프레임을 임의로 합치지 않는 것이 전제다.

이 초안은 Rust `clearra-core-executor`의 `wasm_cpu` 경로에 연결했다.
일반 빌드에서는 제외되며, `local-search-ab` feature로 빌드한 호스트 실행에서
`CLEARRA_PLACEMENT_DEPENDENCY=1`로 활성화할 수 있다. 테스트는 환경 변수 대신
스레드별 scoped mode를 사용한다. 테스트의 worker 1은 A/B 격리를 위한 fixture
설정이며 제품의 worker 선택 정책 변경이 아니다.

기존 `reachability_local_relation.rs`의 검사 셀 추출기를 재사용한다. 새로
추가한 부분은 canonical placement와의 연결, ILC 조건 공유, 삭제 행 조건의
특수화, 보드와 무관한 목표 배치 영향 범위, 기존 역방향 탐색에서 얻은 조건을
저장하고 사용하는 경로다. 새 global pruning 권한이나 공급 순서 축약은 없다.

## 타일링 전에 만드는 정보

`placement_conditions.rs`는 GeometryCatalog가 완성된 직후, 후보 타일링을
열거하기 전에 다음 정보를 만든다.

| 정보 | 의미 |
| --- | --- |
| Canonical row ID | 종류와 원래 좌표계의 셀 집합으로 구별하는 구체적 배치 |
| Exact ILC domain | 각 삭제 행 집합 D에서 가능한 realization이 하나 이상 존재하는지 |
| Shared domain ID | 완전히 같은 허용 D 비트셋의 공유 ID |
| Cell incidence | 원래 셀을 차지하는 canonical placement ID 목록 |

높이 1–6, Board64 범위에서 최대 64개의 D를 직접 표현한다. 서로 다른
realization의 가능성은 OR로 유지한다. 공유하는 것은 같은 조건식이며,
배치 노드나 realization 목록 자체를 합치지 않는다. 조건 캐시는 기존
instantiations를 읽어 만들어지며, 컴파일 도중 자신을 다시 참조하지 않는다.

삭제된 행에 걸친 이미 실행된 미노의 잔여 셀도 의존성에 포함한다. 앞으로
실행할 수 없는 배치라는 이유로 이미 존재하는 잔여 셀의 영향을 지우지 않는다.
지원하지 않는 입력, 용량 초과, 검증 실패는 기존 계산으로 돌아간다.

## 타일링 선택 이후의 줄 삭제 조건

선택 여부 X와 실행 여부 E를 분리한다. 이 초안에서는 선택된 canonical ID
목록을 고정한 뒤, 각 원래 행의 contributor 집합으로 E에 대한 조건을 만든다.

원래 행 r을 채우는 contributor 집합이 P(r)라면, 초기 셀을 상수로 처리한
완전한 행에 대해서만 다음 조건을 사용한다.

```text
Clear(r, E) = 모든 p ∈ P(r)가 실행되었음
D(E) = Clear(r, E)를 만족하는 원래 행 r의 비트 집합
```

같은 contributor ID 집합을 가진 행들은 하나의 AND 조건과 복수 행 마스크로
표현한다. 같은 종류의 서로 다른 배치는 서로 다른 ID로 남는다. 여러 행이
동시에 지워지는 경우에도 원래 행 ID를 원자적으로 반환한다. 일부만 채워지는
목표 행은 삭제 조건이 되지 않는다.

현재 BuildUp의 `expected_deleted_rows`에 이 계산을 연결했다. APDP의 부분
선택 X에 대한 전파, 선택/되돌리기 때의 dirty 목록 갱신, watcher 카운터를
이용한 실행 E의 증분 갱신은 아직 연결하지 않았다. 따라서 전체 조건 그래프를
타일링 전에 완성했다는 구현은 아니다.

## 킥 영향 범위의 증명

최대 킥 이동 거리만으로 독립을 판정하지 않는다. 중간 이동 경로, 착지 지지,
spawn/sky 진입, 먼저 검사하는 킥 후보의 실패 조건도 결과를 바꿀 수 있다.

`reachability_independence.rs`의 정적 분석은 실제 forward template에서 모든
가능한 이동·킥 간선을 만든 뒤 목표 배치의 선행 상태 폐쇄를 계산한다. 여기에
선행 상태에서 검사하는 모든 킥 후보를 포함한다. 목표에 도달하지 않는 앞쪽
킥 후보도 뒤쪽 후보의 선택을 바꿀 수 있으므로 생략하지 않는다.

정적 마스크 M에 대해 같은 규칙과 목표 배치를 사용하는 보드 A, B가

```text
(A XOR B) AND M == 0
```

을 만족하면 해당 목표 배치의 가능 여부가 같다. 교집합이 있다는 사실은
의존성이 있다는 증명이 아니라 판정을 재사용할 수 없다는 뜻이다.

상향 킥과 긴 이동 경로 때문에 정적 마스크가 보드 전체가 될 수 있다.
그 경우 안전하지만 쓸모없는 조건이므로 더 작은 조건부 증명이 필요하다.
정적 계산기는 보드를 받지 않는다. 실행 경로에서는 이미 전체 탐색이 필요한
경우에 목표별로 지연 생성하며, 모든 후보에 대해 선계산하지 않는다.

## 탐색 결과를 관측해 만드는 조건

실행 순서는 기존 hard-drop 검사, 기존 exact-board cache, 조건 재사용,
기존 primitive 탐색, primitive 결과 관측 순이다. 증명 캐시 miss만으로
추가 BFS를 시작하지 않는다.

| 증명 종류 | 생성 자료 | 재사용 가능한 결과 |
| --- | --- | --- |
| 정적 선행 상태 범위 | 보드와 무관한 이동·킥 template | 목표 pose 하나의 Boolean |
| 역방향 탐색 조건 | 실제 처리한 reverse queue prefix와 검사 조건 | 목표 pose 하나의 Boolean |
| 전체 전방 탐색 조건 | 실제 sky seed와 완전히 닫힌 도달 상태 집합 | 전체 grounded-lock 집합 |

역방향 탐색 조건은 target의 충돌·착지 검사, 처리한 상태의 역방향 이동 후보,
역방향 회전 source와 앞선 킥 후보를 포함한다. 성공 조기 종료의 조건을 다른
배치나 전체 lock family의 부정 결과로 확대하지 않는다. 실패한 역방향 탐색도
폐쇄된 탐색의 해당 목표 결과만 저장한다.

전체 전방 탐색 조건은 도달한 집합에서 나가는 모든 결정과 seed의 점유 검사를
포함한다. 다른 보드가 이 셀들에서 같으면 같은 집합에 진입하고 같은 간선이
선택되며 같은 grounding 결과를 얻으므로 전체 lock 집합이 같다.

각 조건을 cell incidence로 canonical placement 목록에 연결한다. 현재
재사용 비교는 occupancy bit mask로 수행하고, 영향 placement ID 목록을 함께
보관한다. 이 목록을 사용하는 dirty propagation은 다음 단계다.

## 잘못된 prune을 막는 경계

- Catalog owner는 Arc allocation의 동일성을 Weak로 고정하고, 부착 시 종류·
  원래 셀·차원·초기 보드·required cells 전체를 확인한다. 이전 catalog의
  벡터 저장소를 캐시 때문에 계속 보유하지 않으며 Weak의 고정 비용도 계산한다.
- Kick profile이 바뀌면 기존 조건을 버린다.
- 삭제된 원래 행 집합 D가 같아야 재사용한다. D가 바뀌어도 물리 보드가 같다는
  이유로 조건을 적용하지 않는다.
- 목표 하나의 증명과 전체 lock family의 증명은 타입 내 scope로 구별한다.
- 최대 128개 relation, 2,048개 정적 목표 마스크, 128K개 influence ID를 유지한다.
  용량 한계는 exact fallback을 사용하며 결과를 false로 바꾸지 않는다.
- 영향 범위 바깥의 미노라도 줄 삭제를 일으킬 수 있다. 그 경우 frame이 바뀌므로
  기존 증명의 적용 대상에서 벗어난다.
- 결과는 기존 도달성 Boolean 질의를 대체한다. 같은 종류의 공급 분기,
  CountAll multiplicity, 확률의 분모, scoring/replay 증거를 축약하지 않는다.

## 측정에서 확인한 설계 수정

처음에는 조건 cache miss마다 전체 도달성 탐색을 수행했다. 4개 미노 표준
7가방 fixture에서 조건 재사용 138회, 서로 다른 보드 재사용 8회가 발생했으나,
전체 방문 상태는 기존 1,724개에서 27,512개로 늘었다. 재사용 횟수만으로는
이득을 판단할 수 없었다.

이에 기존의 짧은 역방향 탐색 결과를 관측하는 방식으로 변경했다. 전체 전방
탐색은 기존 cache 정책이 필요로 할 때에만 실행한다. 방문 상태 감소는 실행
시간 단축과 별개이며, metadata
컴파일·조건 추출·조건 검색·추가 메모리 비용까지 포함한 성능 검증이 필요하다.

## 검증 결과

Rust 1.98.1, Linux 호스트의 debug test 빌드에서 비교했다. 다음 수치는 작은
정확성 fixture의 결과이며 대표 workload 벤치마크가 아니다.

### 4개 미노 / 표준 7가방

입력은 높이 4, 초기 보드 `0xfc3f_0fc3_f0`, 사용 미노 4개, hold off,
CountAll, worker 1이다. 동일 환경에서 기존 모드와 실험 모드를 실행했다.

| 항목 | 기존 | 최종 초안 |
| --- | ---: | ---: |
| 정규화된 해 | 135 | 135 |
| Build variant count | 2,060 | 2,060 |
| 성공 공급 패턴 | 173 / 840 | 173 / 840 |
| Coverage probability | 0.20595238095238097 | 0.20595238095238097 |
| Total reachability states | 1,724 | 1,693 |
| Peak reachability states | 170 | 170 |

방문 상태는 31개, 약 1.8% 감소했다. 기존 cache를 통과한 조건 조회 218회 중
재사용 9회, 그중 서로 다른 보드 사이의 재사용은 4회였다. 기존 reverse
탐색 결과에서 조건을 209회 만들었고 추가 full BFS는 없었다. 이 fixture에서
whole-family 조건이나 정적 조건은 생성되지 않았으며, 관측형 target 조건만
활용했다. 209는 생성 관측 횟수이고 동시에 209개를 보관했다는 뜻이 아니다.

타일링 전 canonical placement 218개에 필요한 exact ILC domain은 15개였다.
나머지 203개 배치는 동일한 domain 저장소를 공유한다. 타일링 후보별 행 조건
특수화는 456회, 생성된 contributor 조건은 합계 1,744개, 그중 복수 원래 행을
묶는 그룹은 80개였다. 이 값들은 메모리 압축 가능성과 조건 반복을 보여 주며
그 자체가 실행 시간 개선 배수는 아니다.

### 반복 미노, 공급 경로, 줄 삭제

| Fixture | 보존한 결과 |
| --- | --- |
| 빈 10×2 / IIOOO / CountUnique | 정규화 해 4개, coverage 1/1 |
| 같은 입력 / CountAll | 정규화 해 4개, build variants 6,144 |
| current I, hold off | 정규화 해 1개, build variants 2 |
| current I와 held I | 정규화 해 1개, build variants 4; 다른 supply action의 배수 보존 |
| II / `0xfc3f0` | 정규화 해 1개, build variants 4; replay 두 단계가 각각 한 줄 삭제 |
| scored I | 정규화 해 1개, build variants 2; 실제 scoring batch와 replay 일치 |

단순 fixture들은 hard-drop에서 해결되어 새 도달성 조건 조회가 0회다.
이 경우에도 ILC domain과 행 조건의 연결은 실행된다. 전체 placement identity,
coverage bitset, 해별 coverage와 probability, replay, scoring batch,
completeness/truncation과 패턴 가중치를 기존 결과와 비교한다.

### 독립성·경계 검증

- 새 테스트 33개와 기존 도달성 회귀 테스트 29개가 통과했다.
- 기존 외부 생성 pack이 필요한 테스트 2개는 원래의 ignored 상태다.
- SRS90, SRS+, SRS-X, Jstris180, NoKick의 5개 profile을 사용했다.
- 전체 lock family 조건의 영향 범위 바깥에서 한 셀을 바꾼 보드 479개를 독립
  BFS와 비교했다. 35개 profile/piece 조합 중 27개가 실제 바깥 셀을 가졌다.
  나머지 8개는 전체 보드 마스크여서 재사용 실험의 성공 사례로 세지 않았다.
- 역방향 target 조건은 영향 범위 바깥 셀 변화에 대해 1,142회 독립 oracle과
  비교했다. 각 profile에서 실제 성공과 exhaustive negative 사례를 요구했다.
- O shelf fixture는 5개 source family로 2,400개 질의를 재사용했고, 1,200개는
  보드가 달랐다. 이것은 제어된 증명 실험이며 일반 PC의 성능 수치가 아니다.
- J를 추가하면 T의 SRS+ 킥이 가능해지는 반례, 앞선 킥이 선행 폐쇄 바깥에
  있는 반례, 원래 행 frame 변경, owner/profile 변경, 캐시 포화, stale scratch,
  불완전 negative, 다른 목표 pose로의 오용, obsolete owner 수명을 검사했다.

재현 명령:

```sh
cargo test --locked -p clearra-core-executor --lib placement_conditions:: -- --nocapture
cargo test --locked -p clearra-core-executor --lib backend::wasm_cpu::reachability:: -- --nocapture
cargo test --locked -p clearra-core-executor --lib placement_dependency_integration_tests -- --nocapture
cargo fmt -p clearra-core-executor -- --check
cargo check --locked -p clearra-core-executor --target wasm32-unknown-unknown
cargo check --locked -p clearra-core-executor --features local-search-ab --target wasm32-unknown-unknown
cargo clippy --locked -p clearra-core-executor --lib --features local-search-ab,parallel
```

기본 및 실험 feature의 wasm32 컴파일, format 검사를 수행했다. 브라우저에서
WASM을 실행한 검증은 아니다. Clippy 일반 실행은 완료했으며 기존
`search_prune_policy.rs`의 precedence/needless_return 경고 10개가 남아 있다.
`-D warnings`를 추가한 엄격한 Clippy gate는 이 기존 경고 때문에 통과하지
못한다. 기본 lib test 빌드의 기존 parallel 관련 dead-code 경고 3개도 남아 있다.

## 다음 단계의 우선순위

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/catalog.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, 구현 내용: 기존 cell-support CSR을 공유해 중복 저장을 줄이고, 동일한 ILC domain과 contributor 조건을 후보 사이에서도 공유한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs`, 구현 내용: 영향 placement 목록과 행 이벤트에 역색인을 두어 X의 선택 변경과 E의 실행 변경을 각각 증분 처리한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/reachability_dependency_cache.rs`, 구현 내용: 서로 다른 D의 잔여 셀·좌표 변환·진입 경계가 같다는 별도 증명이 있을 때만 frame 간 재사용을 추가한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs`, 구현 내용: APDP 부분 타일링에서 확정 가능한 필요조건의 producer API를 만들고, 미확정 상태는 Unknown으로 유지한다. 실제 APDP 소비 경로 연결은 producer 계약 검증 이후 진행한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/placement_dependency_integration_tests.rs`, 구현 내용: 반복 미노·홀드·줄 삭제·각 profile의 정확성 corpus를 넓힌 뒤 관리된 벤치마크로 전체 시간과 peak memory를 비교한다.

Native C 경로, 브라우저 실실행, 전체 7개 미노 PC corpus 성능 검증, 제품 기본
활성화와 배포 게이트 승격은 이 초안의 완료 범위에 포함하지 않는다.
