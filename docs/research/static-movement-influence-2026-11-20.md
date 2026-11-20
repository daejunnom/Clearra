# Clearra: 이동 규칙으로 증명하는 배치 영향 범위와 O 사전 관계

작성일: 2026-11-20 (Asia/Seoul). 실험 브랜치: prototype/placement-dependency-20261120.

## 1. 범위와 결론

이번 변경은 공급 표현의 비트 수보다, 물리적으로 무관한 상태를 구분하고 같은 배치 가능성을 다시 계산하는 비용에 우선순위를 둔다.

구현한 경로는 다음 세 가지다.

1. 실제 O shape와 킥 오프셋의 성질을 검증한 뒤, O의 일반 pose 탐색을 행별 비트 연산으로 대체한다.
2. 다른 미노는 목표에 영향을 줄 수 있는 셀을 점유 보드와 무관하게 한 번 계산한다. 그 범위 밖의 차이를 제거한 키로 정확한 단일 배치 결과를 재사용한다.
3. 고정된 줄 삭제 좌표계에서 O의 최소·최대 점유 보드로 모든 중간 상태의 배치 가능성을 증명한다. 이 증명은 이전 도달성 관측 없이 poset compiler에 전달한다.

실험 flag는 기본 off이며, 일반 배치 전체의 도달성을 제거했다고 주장하지 않는다. 증명하지 못하는 경우에는 기존 정확 탐색을 유지한다. 35비트 공급 표현은 이번 변경에 포함하지 않았다.

정확성 및 작은 제품 예제의 계산 생략은 검증했다. 전체 P7P4는 같은 9워커 조건의 세 실행이 모두 SIGKILL로 끝나 완료 결과를 얻지 못했다. 따라서 이번 변경의 전체 P7P4 시간 단축률과 대규모 적용률은 미확정이다. 중단까지 걸린 시간을 완료 시간으로 사용하지 않는다.

구현 소스 tree는 123be269def12222ad52731c13fd5d2d0a09fdf7이다. 같은 tree를 가진 로컬 소스 commit은 d70832488cdb39b0538163e63244c9ed0bd4ddc9, GitHub 소스 commit은 [bea0383106c9ef37aa11bb5534a1f033396be9b5](https://github.com/daejunnom/Clearra/commit/bea0383106c9ef37aa11bb5534a1f033396be9b5)다. 이후 보고서 추가는 실행 코드 변경이 아니다.

## 2. 무관함의 정확한 의미

목표 배치를 q, 물리 보드를 B, 관찰할 셀 집합을 M(q)라 하자. 필요한 성질은 다음이다.

> B₁과 B₂의 M(q) 안 점유가 같으면, 같은 이동 템플릿에서 q의 정확한 lock 가능성도 같다.

이 성질이 증명되면 다른 미노 p의 셀 집합이 M(q)와 만나지 않는 경우, p의 점유 변화는 q의 배치 판정에 영향을 주지 않는다. 따라서 해당 차이를 보드별 탐색으로 넘길 필요가 없다.

이 판단은 다음 조건을 함께 보존한다.

- 같은 킥 테이블, 정규화된 shape, 목표 위치와 회전, 필드 크기.
- 목표의 grounding 조건.
- 배치 관계로 승격할 때 같은 원래 행 삭제 상태 D.
- 같은 종류라도 서로 다른 배치 operation의 정체성.

특히 “p의 셀이 M과 만나지 않는다”는 사실만으로 p가 일으키는 줄 삭제까지 무관하다고 할 수 없다. 행 삭제는 목표와 경로의 좌표계를 바꿀 수 있다. 물리 보드의 단일 질의 캐시는 동일 물리 입력을 비교하지만, 원래 배치들 사이의 관계는 D와 ILC 대응을 따로 보존한다.

### 2.1 O의 한 단계 범위와 전체 진입 경로

O가 평행 이동 없는 회전과 좌우·하강만 사용하는 경우, 한 번의 충돌 판정은 국소적이다. 다만 최종 목표 주위의 고정된 두 칸만 보면 전체 진입 가능성을 놓칠 수 있다.

실제 회귀 예제는 10×6 필드의 O 목표 (0,0)이다. y=2의 x=0..7을 지붕으로 막고 x=8..9에 두 칸 통로를 남기면, O는 오른쪽으로 들어와 지붕 아래에서 왼쪽으로 이동할 수 있다. x=8, y=2..5의 세로 I를 추가하면 먼 통로가 닫혀 목표가 불가능해진다. 목표 주변은 동일하며 완성된 행도 없다.

따라서 구현은 고정 반경을 선언하지 않고, 목표로 연결되는 실제 수평 구간을 행별로 계산한다. 이 과정에서 닿지 않는 구간과 고정된 경계 너머를 제외한다.

## 3. O: pose queue와 visited 배열 없는 정확 판정

[monotone_reachability.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/monotone_reachability.rs)의 admission은 프로필 이름만 믿지 않는다.

- O의 네 회전 shape가 실제로 동일한 정규화 2×2인지 검사한다.
- 활성화된 O 회전의 모든 킥 오프셋을 정규화한 뒤 이동량이 (0,0)인지 검사한다.
- SRS90, SRS+, SRS-X, Jstris180, NoKick을 각각 검증·캐시한다.
- ASC, ARS, Imported, Custom은 현재 이 경로에 들어오지 않는다. 미래 프로필을 이름만으로 승인하지 않는다.

보드 각 행의 점유 비트를 b_y라고 할 때, O의 왼쪽 아래 anchor가 놓일 수 있는 x의 집합은 다음 비트식이다.

F_y = anchor_mask AND NOT((b_y OR b_(y+1)) OR ((b_y OR b_(y+1)) >> 1)).

목표 anchor에서 시작해 F_y의 연결된 수평 구간을 구하고, 같은 x를 바로 위 행으로 전달한다. 이는 실제 하강 경로를 역방향으로 계산하는 것이다. 위로 움직이는 게임 조작을 추가한 것이 아니다. 모든 유효 x/rotation을 필드 위 source로 허용하는 기존 primitive의 sky 계약을 사용한다.

수평 연결은 1, 2, 4, 8칸의 비트 전파로 계산한다. 중간 anchor가 전부 비어 있는지를 함께 보존하므로 장애물을 건너뛰지 않는다. 현재 admission인 높이 1..6에서는 최대 6개 물리 행만 평가하며, heap pose queue와 visited 상태 배열을 만들지 않는다. 별도로 기록하는 row_steps는 일반 primitive visited states에 넣지 않는다.

목표 아래 두 support 셀 또는 바닥도 확인한다. 받침이 없는 경우에는 행 경로를 만들기 전에 정확한 negative lock 결과를 반환한다. 이 negative는 단일 물리 목표를 완전히 판정한 결과이며, 아래 구간 증명의 실패와는 의미가 다르다.

### 3.1 질의별 영향 범위

각 행에서 다음 셀을 보존한다.

- 들어온 anchor들의 충돌 셀.
- 도달한 수평 연결 구간의 충돌 셀.
- 구간 바로 옆의 막힌 경계를 판정한 셀.
- 목표의 support 셀.

이 셀의 점유가 같으면 다음 행으로 전달되는 구간도 같다는 귀납으로 무관함을 보인다. 실패한 행 위쪽이나, 목표 아래 support 이외의 셀은 고정 물리 좌표계에서 제외할 수 있다.

이는 충분한 영향 범위다. 모든 입력에서 유일하게 가장 작은 셀 집합을 구했다는 주장은 하지 않는다. 이 동적 O 범위를 일반 정규화 캐시에 다시 넣는 최적화는 구현하지 않았다. 현재는 작은 행 연산 자체로 답한다.

## 4. 킥이 있는 미노의 정적 영향 범위

[reachability_independence.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_independence.rs)는 점유와 관계없이 가능한 이동 간선의 그래프에서 목표로 이어질 수 있는 자세 집합 R을 구한다.

관찰할 셀은 다음의 합집합이다.

1. R의 모든 자세가 차지하는 셀.
2. R 안 자세에서 회전할 때, R 안으로 들어가는 마지막 킥 후보까지의 모든 선행 후보가 차지하는 셀.
3. 목표의 grounding 셀.

두 번째 항이 first-success 킥 의미를 보존한다. 앞 후보가 새로 가능해지면 뒤 후보를 선택하지 않게 될 수 있으므로, 앞 후보가 R 밖으로 나가더라도 그 충돌 검사는 남긴다. 반대로 R로 들어가는 마지막 후보보다 뒤의 후보는 목표 경로 선택에 영향을 주지 않는다.

R 밖으로 나가는 독립적인 좌우·하강 간선, R과 관계없는 sky source까지 모두 포함하던 범위를 줄였다. R 밖에서 목표로 돌아올 수 있다면 그 자세는 정의상 처음부터 R에 속해야 한다.

단일 킥의 최대 이동 거리만으로 전체 경로 범위를 정하지 않는다. 연속 회전·이동과 상승 킥 때문에 목표 아래나 멀리 떨어진 곳이 여전히 중요할 수 있다. 따라서 결과가 전체 필드인 경우도 정상이며, 이 경우 추가 정규화 캐시를 사용하지 않는다.

이 정적 컴파일에는 board-independent pose topology의 순회가 남는다. 이는 목표·템플릿별 선행 계산이며, 보드마다 primitive 도달성을 다시 순회하는 것과는 구별한다.

### 4.1 반복 비용과 캐시 배치

[reachability_static_influence.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/reachability_static_influence.rs)는 다음 키를 사용한다.

> 같은 불변 템플릿 owner + piece + target pose + (board AND influence_mask).

정적 영향 범위는 템플릿에 있는 목표별 OnceLock으로 공유한다. 정확한 positive 또는 negative 단일 lock 결과만 캐시에 넣고, 이를 전체 lock family의 exhaustive 결과로 승격하지 않는다. 실패한 hard-drop 하나만으로 negative 결과를 넣지도 않는다.

캐시 항목은 16바이트, 네 항목의 set은 64바이트 정렬이다. 256 set의 항목 배열은 워커당 16KiB로 제한한다. owner bookkeeping과 공유된 proof slot 등은 별도 메모리이며 resource accounting에 포함한다. “총 추가 메모리가 정확히 16KiB”라는 뜻은 아니다. 기존 full-board exact cache를 가장 먼저 확인한다.

35비트 공급 표현이나 모든 배치 쌍의 인접 행렬을 추가하지 않았다. 기존 공급 도메인과 긴 공급 처리 계약을 유지한다.

## 5. 이전 관측 없는 O의 구간 증명과 줄 삭제

고정된 D에서 가능한 prefix 보드가 B_min ⊆ B ⊆ B_max라고 하자. O의 admission이 성립한다면 다음은 충분조건이다.

> B_min에서 목표 grounding이 보장되고, B_max에서 목표 진입 경로가 존재하면, 그 사이 모든 B에서 O의 lock이 가능하다.

B_max의 경로는 장애물을 줄여도 사라지지 않는다. B_min의 받침은 장애물을 더해도 사라지지 않는다. 두 조건을 서로 다른 경계에 적용하는 것이 핵심이다. B_max에서만 확인한 받침을 B_min에도 있다고 간주하지 않는다.

일반 SRS 미노에 이 단조성을 그대로 적용하지 않는다. first-success 킥 때문에 장애물 제거가 원하는 뒤쪽 킥의 선택을 막을 수 있다.

[placement_poset.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs)와 [buildup.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs)는 원래 행 contributor, 선행 배치, 미삭제 행 조건으로 min/max를 만들고 같은 D로 압축한다. O proof는 다음을 추가 확인한다.

- minimum ⊆ maximum.
- frame의 목표 높이와 살아남은 행 용량.
- ILC realization의 정확한 네 셀이 O target mask와 일치.
- 최소 보드의 받침, 최대 보드의 정확한 경로.
- workspace에 남아 있던 프로필이 아닌 현재 problem의 명시적 킥 프로필.

단일 O 질의는 폭 2..10을 지원하지만, 현재 LocalRelationRowFrame의 계약은 10열 전용이므로 구간 API도 이를 유지한다.

구간 증명의 false/None은 Unknown이다. 기존 positive certificate를 확인한 뒤 그것도 없으면 기존 정확 BuildUp으로 돌아간다. candidate가 불가능하다는 prune으로 사용하지 않는다.

성공한 poset은 기존 piece-order language와 공급 계산으로 연결한다. 같은 kind operation은 물리 증명 단계에서 따로 유지한다. CountAll에서는 기존 순서/hold 선택의 다중성을 보존하고, 새 단일 lock 계산만 재사용한다.

### 5.1 통계의 해석

- algebraic_interval_attempts/guards/row_steps는 이후 다른 조건에서 candidate 증명이 실패한 경우의 비용도 포함한다.
- skipped_build_graphs만 최종적으로 그래프를 건너뛴 candidate 수다.
- cached_interval_guards는 완료된 poset에 쓰인 기존 관측 증명 수다.
- 영향 범위의 ignored_cells는 질의별로 제외한 셀 수의 합계다. 서로 다른 미노 관계 수가 아니다.
- worker가 새 진단 8개를 모두 생략하면 미보고 기여를 0으로 합산한다. 일부만 보내면 거절하고, 전부 보내면 canonical 정수를 엄격히 검사한다. 기존 필수 correctness evidence는 그대로다.

## 6. 정확성 검증

전체 CI를 대체하는 주장은 하지 않는다. 이번 변경의 primitive, 관계 증명, 실제 제품 확률, 계측 집계를 검증했다.

| 검증 묶음 | 결과 |
| --- | --- |
| reachability 모듈 | 71 passed, 2 ignored |
| static 영향 및 제품·진단 | 10 passed |
| 기존 structural graph/probability 회귀 | 5 passed |
| worker/result 계측 합산 | 2 passed |
| 실험 feature 없는 cargo check | 통과 |
| release benchmark 빌드 | 통과, 2분 27초 |

static 영향 묶음의 캐시 시험 둘은 reachability 묶음에도 들어가므로 위 행들을 고유 테스트 수로 단순 합산하지 않는다. ignored 둘은 외부 immutable pack의 명시적 다운로드가 필요한 기존 시험이다.

O의 행 계산은 작은 필드의 모든 점유, 모든 회전·목표와 5개 프로필에서 독립 primitive oracle과 비교했다. 수평 비트 전파는 9 anchor의 모든 free/seed 조합으로 검증했다. 영향 범위 밖 변화, 먼 진입로 차단, first-success 선행/후행 킥, 잘못된 frame/ILC mask, 구간의 16개 중간 점유도 포함한다.

### 6.1 제품 수준의 실제 해별 검산

혼합 4미노 fixture는 Include 모드로 모든 해 identity, 전체 및 해별 coverage, 개별 확률, 보존한 replay와 완료 상태를 비교했다. TLS 실험 모드를 같은 스레드에서 적용하는 작은 단위 fixture에만 worker=1을 사용했다. 큰 벤치마크의 Auto worker 정책과는 다르다.

| 킥 프로필 | 해 수 | 성공 큐 / 840 | primitive states off→on | O 정확 답변 | O 행 평가 |
| --- | ---: | ---: | ---: | ---: | ---: |
| SRS90 | 135 | 172 | 921→707 | 22 | 56 |
| SRS+ | 135 | 173 | 1,114→911 | 22 | 56 |
| SRS-X | 149 | 200 | 1,164→940 | 22 | 56 |
| Jstris180 | 135 | 173 | 975→910 | 22 | 56 |
| NoKick | 117 | 136 | 582→371 | 22 | 56 |

각 프로필의 정확 답변은 positive 6, negative 16이다. 이 작은 fixture의 일반 영향 범위 캐시 hit는 0이다.

같은 혼합 문제의 CountAll에서는 135개 해별 coverage와 총 2,060개 build variant가 모두 일치했다. primitive states는 1,693→1,238, O 답변 34개, O 행 평가 104회다. 현재/hold가 모두 O인 경우의 다중성, IIOOO, 두 O가 중간에 각각 두 줄을 지우는 경우도 비교했다. 마지막 세 예제는 모두 hard-drop 경로여서 새 O 행 계산의 성능 근거로 삼지 않는다.

### 6.2 Cold O 관계가 실제 그래프 생성을 생략하는 예제

높이 3의 공동에서 O는 (0,0),(1,0),(0,1),(1,1), L은 (2,0),(2,1),(2,2),(1,2)를 채운다. 나머지 셀은 초기 점유다.

- O를 먼저 놓을 때는 (0,2)의 지붕 때문에 바로 떨어질 수 없지만, x=1 통로로 내려와 왼쪽으로 이동할 수 있다.
- L을 먼저 놓으면 원래 2번 행이 지워지고 O가 바로 내려올 수 있다.
- D=0에서 L은 그 행의 유일한 contributor이므로 미삭제 조건이 L의 부재를 강제한다. 이 조건으로 O의 최대 점유 보드를 줄인다.

제품 탐색은 이 배치를 포함해 5개 해, 42개 중 5개 성공 큐를 찾았다. 모든 해별 coverage와 확률이 일치했다. 대표 trace를 만들지 않는 CoverageSummary에서도 실제 해별 coverage로 확률을 재구성해 비교했다.

| 지표 | 기존 경로 | 새 경로 |
| --- | ---: | ---: |
| BuildUp graph bypass | 0 | 1 |
| 배치 순서 graph nodes | 21 | 17 |
| primitive visited states | 87 | 74 |
| 기존 관측 interval guard | 0 | 0 |
| 새 O interval proof | 0 | 1 |
| 새 interval의 행 평가 | 0 | 3 |

이 예제는 이전 primitive 경로를 관측해 둔 결과를 재사용한 것이 아니다. 새 구간 증명은 cold workspace에서 성공했다.

## 7. 큰 P7P4 측정

### 7.1 비교 계약

빈 10×4 field, 4줄 PC, 10개 배치/11개 공급(P7P4), 빈 hold, Jstris180, CountUnique, full-future-oracle를 사용한다. native host의 Rust WasmCpuSearchBackend 경로다. C core, 브라우저 WASM, GUI, CLI의 측정으로 일반화하지 않는다.

tablebase, exact legal-board, conditioned reachability 가속은 off다. Auto worker 9개와 use_all_logical_processors=true를 유지한다. 사용자의 12논리 프로세서/약 4분 결과나 이전 turn의 다른 바이너리 시간을 이번 speedup 계산에 섞지 않는다.

| 군 | placement dependency | structural order | static influence |
| --- | ---: | ---: | ---: |
| A | 1 | 1 | 0 |
| B | 1 | 1 | 1 |

이 비교는 기존 구조적 증명 경로에 O 행 계산, 정규화 캐시, cold O guard를 추가하는 효과를 측정한다. 이번에 축소한 정적 영향 범위 함수 자체는 A/B에 공통이므로, 이전 commit 대비 모든 변경의 합계 효과를 격리한 비교는 아니다.

두 군은 SHA-256 2df61eb5464cdf3a34e4ad414899e65f0c7062f52b9c517041625bd9dc2aa0d3인 같은 release 바이너리를 사용한다. 빌드 receipt는 1795138160572949178-5, 작은 실행 preflight는 1795138407782392029-5다.

실행 순서는 A1 → 동일 조건 A1 재실행 → B1이다. 예정했던 A→B→B→A의 완료 실행 비교는 성립하지 않았다. 기준군의 두 번과 새 경로의 한 번이 모두 같은 종류의 종료를 보였으므로, 같은 큰 실행을 더 반복하지 않았다. 워커 수·메모리 정책·문제 크기를 줄여 결과를 대체하지 않았다.

### 7.2 미완료 실행 기록

| 실행 | 새 기능 | manager 관측 지속시간(초) | 관측 최대 프로세스 그룹 RSS(bytes) | 자식 종료 | receipt ID |
| --- | --- | ---: | ---: | --- | --- |
| A1 | off | 131.868 | 4,735,586,304 | 137 / SIGKILL | 1795138412761039705-5 |
| A1 재실행 | off | 117.915 | 4,738,732,032 | 137 / SIGKILL | 1795138605326982383-5 |
| B1 | on | 114.732 | 4,642,533,376 | 137 / SIGKILL | 1795138770020171730-5 |

세 실행 모두 결과 JSON이 비어 있고 manager reason=nonzero, memory pressure events=0, timeout=900초였다. 위 시간은 관리자가 관측한 미완료 프로세스 지속시간이다. 검색 완료 시간, speedup, p95 또는 신뢰구간으로 해석할 수 없다. RSS 역시 약 2초 간격의 표본 최댓값이어서 종료 직전의 짧은 상승까지 배제하지 못한다.

이 환경에는 읽을 수 있는 cgroup memory.events가 없고 현재 프로세스 namespace에서 커널의 OOM 사건을 확인할 수 없었다. 137과 RSS만으로 kernel OOM, 외부 종료 또는 인프라 원인을 구분하지 않는다. manager 자체의 pressure 종료로도 기록되지 않았다.

소스 소유권 검토에서는 새 메모리가 candidate 수에 비례하여 GB 단위로 쌓이는 경로를 발견하지 못했다. static0의 새 캐시는 비어 있고, 통계는 workspace/worker/session당 고정 스칼라다. 기존 symbolic cache의 재활용 기준과 공급/언어의 구현은 이번에 변경하지 않았다. 다만 공통 정적 영향 범위 변경이 기존 cache hit와 worker 작업 분배에 간접 영향을 줄 가능성까지 배제한 것은 아니다. 이 검토를 종료 원인의 증거로 삼지 않는다.

기존 fixture가 요구하는 456,459개 canonical 해, cts1:2fcbc950acdffa31, 4,233,600개 큐의 coverage 등은 runner의 검산 기준이다. 이번 큰 실행에서는 그 결과까지 도달하지 못했으므로 이 수치들을 새 바이너리의 대규모 정확성 통과 결과로 보고하지 않는다. 큰 탐색은 해별 확률 Omit 설정이므로, 완료하더라도 전체 해 집합과 coverage 합집합 검산이며 개별 해의 모든 확률 검산은 아니다.

### 7.3 재현과 원본 기록

release 빌드는 cargo build --release --locked -p clearra-core-executor --features parallel,local-search-ab --example placement_graph_bench로 만든다.

관리자 아래에서 세 환경 변수를 설정한다. A는 CLEARRA_PLACEMENT_DEPENDENCY=1, CLEARRA_STRUCTURAL_ORDER=1, CLEARRA_STATIC_INFLUENCE=0이고 B는 마지막 값만 1이다. 실행 명령은 build/cargo/default/release/clearra-manage runtime run --producer benchmark --profile benchmark-search --timeout 900 -- build/cargo/default/release/examples/placement_graph_bench다. 자동 CPU 정책은 실제 환경의 논리 프로세서 수를 그대로 사용한다.

이번 실행은 상위 shell이 자식의 종료 상태를 stderr에 추가하도록 감싸 SIGKILL을 137로 보존했다. raw JSON·stderr·test log·이번 시점의 GitHub 도구 89개 목록은 _local/artifacts/static-influence-20261120/에 보관한다. 원시 출력은 이 문서로 복사하지 않았다.

검증 receipt는 reachability 1795137829598485400-5, 최종 제품·진단 1795138081169823997-5, 기존 structural 회귀 1795137982764134560-5, 계측 합산 1795137986598308157-5, 일반 설정 cargo check 1795137992333873676-5다. 모두 정상 종료했다.

## 8. 다음 구조 개선의 기준

1. 목표별 influence mask를 candidate의 구체적인 operation 집합에 사상해, 조건식에서 무관한 operation 변수 자체를 제거한다. 현재 정규화 캐시는 동일 답변의 재계산을 줄이지만, 일반 BuildUp의 모든 순서 상태를 없애지는 않는다. 물리 관계식의 변수 수를 줄이는 것이 다음 핵심 목표다.
2. O의 행별 연결 구간과 경계 증명을 타일링 단계의 반복 목표에서 더 넓게 재사용한다. 독립성이 확인된 미노의 추가/제거는 다시 판정하지 않고, 경계와 만나는 변경에 대해서만 증분 갱신한다. 여러 목표가 같은 보드에 묶이는 경우에는 한 번의 행 전파로 모든 O lock을 얻는 경로도 검토한다. 이를 full-family exhaustive 결과로 표시하려면 실제로 그 family 전체를 계산한 증명이 필요하다.
3. 고정 D의 미삭제 행 조건으로 max에서 빠질 contributor를 더 일찍 계산하고, D가 바뀌면 영향 범위를 다시 투영한다. 줄 삭제는 별도 의존성 사건으로 취급한다.
4. 킥이 있는 미노는 first-success 후보 prefix를 보존한 조건부 범위 축소를 검토한다. 최초의 미확정 원인과 실제 영향을 주는 contributor를 재사용 가능한 키로 묶어 실패 candidate의 반복 증명 비용도 줄인다. 전체 필드가 영향을 주는 목표에 작은 반경 prune을 억지로 적용하지 않는다.
5. 물리 그래프를 생략한 후보에서만 기존 scalar poset 분해·언어 합집합·공급 계산을 적용한다. 일반 그래프의 선형 확장 수를 성공 큐 수와 동일시하지 않는다. hold, 같은 kind, 여러 tiling의 성공 큐 중복은 계속 정확하게 처리한다.

35비트 전용 공급 키를 위 작업보다 먼저 적용하지 않는다. 공급 크기가 커지는 경우의 도메인과 cache line 비용을 유지하면서, 재사용 가능한 관계 및 증분 갱신이 실제 반복 횟수를 얼마나 줄이는지부터 측정한다.
