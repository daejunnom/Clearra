# P7P4 기존 경로·셀 DAG 비교와 메모리 수명 개선

기록일: 2026-11-24 KST. 원시 실행 시각은 UTC로 보존한다.
비교 기준 source는 `671584db1d1c5b023ee1d82fbbf72421654c4d0e`, tree는
`2251110a3092d40c77c4c9f837319cec445b67e7`이다.

## 1. 같은 바이너리의 실제 비교

**기존 알고리즘은 완료했지만 셀 DAG 기준선과 후속 후보는 모두 완료하지 못했다.**
완료시간 speedup이나 Full P7P4 결과 동등성은 확보하지 못했다. 관측된 최대 RSS는
셀 DAG 쪽이 더 컸고, 이번 좁은 개선으로 전체 시간·Peak RSS가 개선됐다는 결과도
얻지 못했다. 후속 후보의 상세 검증과 실패 관측은 §6에 기록했다.

| 측정 | 기존 알고리즘 | 셀 DAG 기준선 |
| --- | ---: | ---: |
| 비교기 선택 | `legacy` | `symbolic-cells --archive-only` |
| 탐색 완료 | 예 | 아니오 |
| 탐색 시간 | 106.219981005초 | 완료시간 없음 |
| Manager 전체 수명 | 107.103초 | 647.128초 후 signal 9 |
| GNU Time 경계 수명 | 107.13초 | 647.29초 |
| 종료 시 커널 최대 RSS | 4,549,275,648 B / 4.236843 GiB | 8,483,328,000 B / 7.900715 GiB |
| 자식의 1초 표본에서 최대 VmHWM | 4,549,275,648 B | 8,327,917,568 B |
| Manager 2초 표본 process-group RSS 최대 | 4,505,661,440 B | 8,117,989,376 B |
| 정규화된 identity 출력 | 456,459개 완료 | 미완료 |
| 마지막 관측 | 탐색·출력 완료 | rank 4, O, `piece_down` |
| 감독 프로그램 종료 요청 | 없음 | 없음 |
| 자동 재시도 / 시간 제한 | false / 0초 | false / 0초 |

관측된 커널 최대 RSS 비율은 **1.8647645595배**다. 이것은 도달 단계가 다른 두
실행의 관측 peak 비율이며, 동일 완료 작업량의 효율 비율이 아니다. 실패 수명을
기존 완료시간으로 나눠 speedup 또는 slowdown으로 제시하지 않는다.

두 arm은 같은 보존 바이너리
`_local/artifacts/symbolic-family-compare-baseline-20261124.bin`을 실행했다.
SHA-256은 `e8a704ddaacd77a95a5c8f7e6bd67cfd4499b219eef4a18e9ff7b933bb29d40a`다.
Rust 추출기가 실제 `compiled_request`와 원래 `source` JSON을 각각 비교해 일치를
확인했다. Request SHA-256은
`e709af430f47ba7292eba224b3af7414efaa13560082dc77c474089992afa924`, source SHA-256은
`989d0aa152b2a75913d02142fcca6df8d6e88302de116c84060fb22decc81d56`이다.

### 요청과 비교 범위

빈 10×4, 정확히 10 locks, Jstris 180, P7P4의 원래 11-draw source,
초기 Hold 없음·Hold 허용·FullQueueOracle·CountUnique·Summary 조건이다.
원래 source는 4,233,600개다. Auto 정책은 허용된 논리 CPU 9개를 사용한다.
탐색 node/candidate/frontier/pattern/time 상한은 0, memory 상한은 None이다.
비교 harness의 CPU/GPU warmup, tablebase, exact-legal-board,
conditioned-reachability, precompute-build-dependencies는 양쪽 모두 false다.
기존 `max_results=256` 출력 계약을 변경하지 않았다.

`legacy`는 이 공통 비교 정책으로 production 알고리즘 진입점을 실행한다. 제품의
모든 기본 가속 옵션을 그대로 켠 별도 제품 benchmark로 설명하지 않는다.
셀 경로는 원래 identity×source 관계를 내부에 보유하며, 기존 경로의 저장 결과는
identity 집합과 global source coverage다. 이 내부 표현·보유 범위의 차이도 메모리
해석에 포함한다. `archive-only`는 완료 후 CTK4를 저장하는 옵션이며 탐색을
축소하지 않는다. 이번 셀 실행은 archive 저장 경계에 도달하지 않았다.

알려진 100% 확률을 검색·재증명하거나 별도 확률 계산을 실행하지 않았다.
Full P7P4의 두 완성 결과가 없으므로 count·global coverage만으로 full joint
동등성을 선언하지 않는다.

## 2. 복구 환경과 측정 경계

이전 scratch가 초기화되어 승인된 feature branch를 새로 clone했다. 과거의 ignored
raw 자료·compiler cache는 없고, Git에 보존된 검토 문서는 과거 증거로 남는다.
따라서 과거 실패 시간과 이번 새 환경의 수명을 직접 속도 비교에 섞지 않았다.

- 물리 RAM: 10,451,423,232 B, swap 0.
- 허용 CPU: 0–8, 허용 NUMA node: 0. 다중 NUMA 확장성 실측은 아니다.
- Rust/Cargo: 저장소가 고정한 1.98.1. Release thin LTO, codegen-units=1 유지.
- Build·TMPDIR·탐색 출력: overlay의 저장소 `build` / `_local` 경로.
- 두 arm은 순차 실행했고 다른 무거운 build/search를 겹치지 않았다.
- Worker 생성 완료 barrier·worker 감소·OOM 자동 retry를 추가하지 않았다.

GNU Time은 manager 바깥에서 감싸 managed direct child가 실제 solver인 관계를
유지했다. `%M`은 종료한 command와 기다린 descendants 중 **단일 프로세스의 커널
최대 RSS**이며, 동시 process-tree RSS 합이 아니다. Manager의 표본 process-group
RSS와 자식 VmHWM을 서로 다른 열로 둔 이유다. Legacy의 검색 완료 직전 VmHWM도
4,549,275,648 B로, 출력 단계가 이번 kernel peak를 올리지는 않았다.
Linux `getrusage(RUSAGE_CHILDREN)`의 범위는
[Linux man-pages](https://www.man7.org/linux/man-pages/man2/getrusage.2.html), `%M`의
정의는 [GNU Time manual](https://www.gnu.org/software/time/manual/time.pdf)을 따른다.

### 이번 signal 9와 저장 공간

셀 기준선의 `/proc/vmstat` `oom_kill`은 실행 전 0에서 실행 후 1로 증가했다.
Receipt에는 direct child의 Unix signal 9, `supervisor_termination_reason=null`,
빈 `termination_audit.attempts`, `automatic_retry=false`, `timeout_seconds=0`이
기록됐다. 메모리 압박에 따른 OOM 경로를 강하게 지지하지만, 전역 counter에는
victim PID가 없으므로 별도의 kernel victim 로그를 확보한 것으로 서술하지 않는다.

셀 기준선 전후 overlay 가용 공간은 61,151,649,792→61,131,788,288 B,
free inode는 4,181,518→4,180,653개였다. `/tmp` 사용량은 두 경계 모두 0 B였다.
관측된 ENOSPC나 inode 고갈이 없다. 새 환경에는 다른 프로젝트의 큰 tmpfs 점유가
관측되지 않았다. 다른 프로젝트·공용 cache를 정리하거나 Drive에 업로드하지 않았다.
사용자의 Mosk 정리 요청 확인은
[기존 저장소·프로세스 감사 §13](2026-11-23-storage-process-pressure.md#13-2026-11-24-사용자-확인과-새-비교-환경)에 추가했다.

## 3. 실제 증가 지점

셀 기준선의 geometry compiler 완료 경계는 탐색 시작 후 약 1.472초,
compatibility 인증 포함 경계는 약 1.475초였다. 실패는 이 준비 단계 뒤에 발생했다.
마지막 645.944871초 표본의 값은 다음과 같다. 서로 독립적으로 갱신되는 diagnostic
payload 추정치로, 합계가 순간 RSS와 일치해야 하는 값은 아니다.

| 구조 | 마지막 관측 |
| --- | ---: |
| 중앙 joint ZDD | 36,385,800 nodes / 2,956,130,176 B retained estimate |
| Geometry compatibility memo | 129,238,954 entries / 1,717,573,600 B retained estimate |
| 공유 Boolean prefix | 8,659,546 nodes / 641,728,512 B retained estimate |
| 소비 중 O private suffix | 175,027,175 B retained estimate |
| Published private suffixes | I/T/S/Z/L 각 112,721,920 B, J 175,112,192 B |
| 누적 Down work | 1,914,961,920 |
| 누적 ZDD work | 2,750,939,136 |
| 누적 Boolean work | 2,850,618,223 |
| 누적 bridge work | 13,107,200 |
| 생성한 추가 geometry node | 0 |

Rank 3에서도 중앙 ZDD는 약 8,745만 nodes까지 커졌고, 그 시점 J 하나의 private
suffix는 약 11.6MB였다. Rank 경계의 compaction 뒤 node 수가 줄어도 다음
`piece_down`에서 다시 큰 중간 관계가 생겼다. 그러므로 worker 소유권의 개선만으로
전체 메모리 문제를 해결한다고 볼 수 없다. 반대로 rank 4에는 대기 중인 private
suffix도 상당하므로 이 보유 시간을 줄이는 작업은 별개의 유효한 개선 대상이다.

공통 prefix는 한 번 공유한다. Piece task는 최대 7개라 requested workers=9가
private graph 9개를 뜻하지 않는다. `sync_channel(0)`에 send가 막힌 worker도
완성한 graph를 소유한다. Compute 중인 worker 수만 세면 Published/Consuming
메모리를 놓친다. 서로 다른 piece의 graph 크기도 달라 정확한 상수의 선형식으로
단정하지 않는다. 이번에는 worker 수를 바꾼 scaling benchmark를 하지 않았다.

### 기존 경로의 worker별 보유량

Legacy의 `native-worker-exit-private-retained-payload-sum.v2` 합계는
4,178,574,916 B이며, 이 중 standard-bag storage가 3,582,316,456 B,
piece-language storage가 506,529,972 B다. Shared standard-bag request는
36,424 B로 별도 계측된다. 이 값은 **각 worker 종료 직전 capacity 합계**이며
동시 생존량이나 peak RSS가 아니다.

[`parallel_worker.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/parallel_worker.rs)는
worker마다 workspace를 queue loop 동안 재사용하고, 종료 수치를 기록한 뒤
workspace를 drop한다. 이 scratch가 coordinator로 복사되어 남는 구조는 아니다.
[`standard_bag_coverage.rs`](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs)의
3.58GB는 결과 decision graph·product/union memo·interner와 관련 capacity다.
Cursor transition·suffix count·factorized layout은 이미 요청 단위 Arc로 공유한다.

| Legacy worker 합산 항목 | Entries | Capacity | Retained payload |
| --- | ---: | ---: | ---: |
| Product memo | 24,214,893 | 84,312,096 | 674,496,768 B |
| Union memo | 41,719,592 | 124,780,544 | 1,497,366,528 B |

기존 logical-live 경계에서 original ordinal words로 flush한 뒤에도 재사용 capacity는
남는다. 따라서 해제되지 않은 live node와 빈 capacity를 구분해야 한다. Capacity를
줄이면 이후 재할당·재계산 비용이 생길 수 있어 이 합계를 보고 바로 shrink하지 않았다.
Product key의 language-node ID와 값의 decision ID, Union의 node pair는 worker-local
owner에 속한다. 숫자 ID가 같다는 이유로 memo를 Arc로 공유하면 다른 집합을 같은
것으로 취급할 수 있다. 일반 queue의 pattern position index도 이미 공유되어 있다.
추가 공유에는 정확한 semantic interning과 값 graph의 owner 이관이 함께 필요하며,
중앙 메모리·lock·NUMA 비용으로 옮겨가지 않는지 별도 검증해야 한다.

## 4. 이번 구현 후보

### 이동 자료의 마지막 사용에서 해제

`PieceMovement`는 Live query storage 또는 Released scalar accounting을 가진다.
셀 mode가 Boolean/cofactor 조회를 모두 끝내 ZDD `selected`·`extended` roots만
남긴 직후, private Boolean arena와 legal-row Vec를 해제한다. 이후의 Down,
intersection, 원래 source semijoin이 그 arena의 수명을 늘리지 않는다.

해제는 idempotent이고 마지막 work pulse를 먼저 flush한다. Peak·work·GC 카운터는
scalar로 보존하고 parent accounting에 한 번 반영한다. 오류·취소·channel drain도
같은 해제 경로를 사용한다. Sequential parent와 row-variable mode의 필요한
수명은 그대로 보존한다. 다른 Published owner까지 해제하는 변경은 아니다.

### 이미 포화된 이동 target의 계산 생략

모든 새 source delta가 D 안에 있고 `reached[target]==D`이면,

\[
\Delta\cap\neg reached[target]=\varnothing.
\]

따라서 기존 canonical root 비교 한 번으로 novelty NOT/AND와 guard 계산을
생략한다. Domain 밖에서도 참일 수 있는 collision-free predicate와 혼동하지
않는다. 뒤의 ordered kick은 원래 prior 목록을 계속 읽으므로 이전 kick의
first-success 우선순위를 잃지 않는다. 킥 이름에 따른 특수 분기는 없다.

### 충돌할 변수가 없는 ZDD suffix 전체 생략

각 row r에 대해 `h_r=max {s≠r | cells[s] & cells[r]≠0}`를 구한다. ZDD 자식의
변수는 엄격히 증가하므로 현재 node.variable이 h_r보다 크면 그 아래에는 r과
충돌하는 row가 없다. 이 경우 suffix를 그대로 반환하여 순회·memo·mk를 생략한다.

각 cell의 가장 큰 두 row ID만 보유하면 자기 ID를 제외한 h_r를 계산할 수 있다.
두 번의 cell 순회 비용은 O(Σ popcount(row_cells)), tetromino에서는 O(4R)이고,
추가 persistent storage는 row당 u32 하나다. 현재 R=764에서는 3,056 B다.
동일 footprint의 다른 row ID, source tail, ZDD low==high 의미를 보존한다.
새로운 BDD나 부분집합 family를 먼저 만드는 방식은 아니다.

### 계측과 GC 판단

비교 sampler에 per-rank ownership과 Computing/Published/Consuming/Released별
private payload를 추가했다. Sequential 공유 parent는 합산하지 않는다. Rank·mode를
수집 양 끝에서 다시 읽고 바뀐 경계의 그룹 합은 내보내지 않는다. 모든 값은
비동기 최신 관측이며 simultaneous peak나 실행 완료 권위가 아니다.
기존의 낮은 빈도 storage pulse에서 guard suffix 생략 및 Union remainder의
호출·생성 node·work 카운터를 내보내, 실패해도 비용 근거가 남도록 했다.

추가 per-piece GC는 채택하지 않았다. 할당 node 수만으로 dead 비율은 알 수 없고,
기존 compact는 old/new graph·unique·remap이 동시에 존재한다. In-place 방식도
remap·노드·테이블 순회, cache 무효화와 재계산, atomic commit의 취소 지연이 생긴다.
현재 rank 경계를 임의로 더 자주 실행해 속도·peak를 개선했다고 가정하지 않는다.

## 5. BuildUp 제거의 현재 상태

이번 변경은 BuildUp 자체를 제거하지 않는다. 셀 경로는 여전히
`current → ready → Legal → add → Down → next`의 rank별 순서 존재 계산을 한다.
기존 static poset certificate가 실제 물리 탐색을 생략하는 경우와, 이를 임의의
완성 geometry DAG 전체에 적용할 수 있다는 주장은 구분해야 한다.

완성 geometry·공급·이동 관계에서 순서/Hold/path 보조변수를 존재 양화하는 구조는
rank별 partial family와 반복 Down를 없앨 수 있는 설계 대안이다. 그러나 중간
factor의 변수 폭으로 같은 폭증이 옮겨갈 수 있어, 아직 개선된 구현이라고
채택하지 않았다. `G(U,M)`도 geometry 판정 축약이지 순서 존재의 제거가 아니다.
검토와 정확성 경계는
[BuildUp 구조·worker 메모리 감사](buildup-structure-and-worker-memory-2026-11-24.md)에 남겼다.

## 6. 검증·후속 실측 기록

후보 source commit은 `5c70a989b575ce45892070dc565f68ba39597cf3`, tree는
`1193ab7caeae2f87d6e58eea9b6c7adf47701ec8`이다. Source 변경 10파일을 고정한
뒤 release binary를 만든다. 뒤의 문서 변경은 컴파일된 solver source를 바꾸지 않는다.
연결된 GitHub 경로로 만든 코드 commit은
`6c477bb867ca9e1b3fcfa1e0df7cf7d7530db5d1`이며, API가 반환한 tree SHA가 위의
측정 tree와 정확히 같음을 확인했다. 로컬 Git의 쓰기 인증이 없어서 게시 commit의
작성 metadata와 SHA는 달라졌지만, benchmark가 기록한 로컬 commit 값을 바꾸거나
다른 바이너리를 측정한 것으로 재작성하지 않았다.

### 로컬 correctness 검사

첫 `symbolic_` 필터의 129개 중 127개가 통과했고, 2개는 legacy reference 실행을
시작하기 전에 `SharedResourceContention`으로 거절됐다. 여러 테스트가 각각 전체
CPU lease를 요청한 충돌이며, 완료된 해법이 다르다는 assertion 실패가 아니다.
이 실패 invocation을 성공으로 바꾸거나 숨기지 않았다.

해당 family 통합 모듈만 `--test-threads=1`로 격리해 **9 passed / 0 failed**를
확인했다. 각 검색의 worker 설정은 그대로이고 테스트 harness의 동시 실행만
분리했다. Geometry compatibility 모듈은 **6 passed / 0 failed**다. 앞선 성공을
합치면 서로 다른 135개 검사에서 성공을 확인했다.

| 실행 | 결과 | Receipt |
| --- | --- | --- |
| `p7p4-candidate-symbolic-tests-20261124` | 127 passed, 2 resource-admission failures; build 1분 54초, tests 6.67초 | `1795457001763141419-3` |
| `p7p4-candidate-family-isolated-tests-20261124` | 9 passed, tests 1.45초 | `1795457167128311153-3` |
| `p7p4-candidate-compatibility-tests-20261124` | 6 passed, tests 0.01초 | `1795457257611775244-3` |

검사에는 다섯 builtin kick의 독립 physics 대조, imported ordered-table 구분,
원래 identity×source·Hold 관계, 같은 물리 보드의 서로 다른 삭제 frame, parallel
owner/cofactor 수명, 오류·취소 drain, 정확히 한 번의 work 반영이 포함된다.
추가 suffix fixture는 pairwise conflict oracle과 기존 traversal을 비교하며,
원래 source 상관과 low==high 선택을 보존한다. 이 유한 fixture의 work 감소를
Full P7P4 속도 향상으로 대신하지 않는다.

### Release build와 provenance

동일 release 설정의 후보 build는 Cargo 표시 **3분 16초**에 완료했다. 보존 binary는
`symbolic-family-compare-candidate-20261124.bin`, SHA-256은
`994059e247e42ab6872cd0c80f8924c77041dec092d9fa2f608589470cb31ec9`다.
Build receipt는 `1795457294604460755-3`이다.

두 `--timings` HTML의 `UNIT_DATA`를 Rust로 추출했다. Warm cache가 있어도 변경한
executor의 release code generation과 후속 binary 작업이 긴 상태다.

| Cargo unit | 기준선 cold build | 후보 build |
| --- | ---: | ---: |
| `clearra-core-executor` | 129.85초 | 145.97초 |
| executor frontend 구간 끝 | 29.13초 | 28.41초 |
| `symbolic_family_compare` binary unit | 43.22초 | 50.71초 |

Unit들은 병렬로 겹칠 수 있으므로 단순 합으로 wall time을 만들지 않는다. Binary
unit의 기록에는 frontend/codegen 상세 구간이 없어 50.71초 전부를 linker 비용으로
단정하지 않는다. HTML의 정수 `DURATION`은 그림의 시간축 경계로, 정밀한 wall
측정값 대신 쓰지 않는다. 이 한 번의 cold/warm 대조는 build 최적화 성공의 근거가
아니다. Release LTO·codegen 설정을 낮추는 대신 실제 executor compilation 비용을
후속 분리·재사용 검토 대상으로 남긴다.

### 후속 후보의 실제 P7P4 결과

후보도 시간 상한 없이 원래 worker 정책으로 한 번 실행했으며 **739.252초 후
signal 9**로 종료했다. Failed run의 자동 재시도나 worker 감소를 하지 않았다.
이번 실행은 다른 검증된 source 변경을 평가하는 별도 후보 실행이다.

| 항목 | 후속 후보 |
| --- | ---: |
| Manager 수명 / GNU Time 경계 | 739.252초 / 739.43초 |
| 커널 최대 RSS | 8,470,355,968 B / 7.888634 GiB |
| 자식 1초 VmHWM 표본 최대 | 8,138,760,192 B |
| Manager process-group RSS 표본 최대 | 8,172,433,408 B |
| 마지막 관측 시각·단계 | 737.972949초, rank 4 / O / `piece_down` |
| 중앙 ZDD | 36,398,016 nodes / 2,956,130,176 B retained estimate |
| Compatibility memo | 129,228,880 entries / 1,717,576,656 B retained estimate |
| 공유 Boolean prefix | 641,728,512 B retained estimate |
| Published private suffix 6개 | 합계 738,721,792 B retained estimate |
| O 이동 자료 | Released, private bytes 0 |
| 커널 전역 `oom_kill` | 1→2 |
| Receipt | `1795457530729403796-11` |

기준선과 후보의 실제 compiled request/source는 다시 비교해 같음을 확인했다.
Source commit/tree와 binary는 의도적으로 다르다. Supervisor 종료 요청은 없고,
Unix signal 9와 빈 termination-attempt 배열, timeout 0, automatic retry false를
기록했다. Overlay 가용 공간은 55,955,279,872→55,936,163,840 B, free inode는
4,176,716→4,176,602개이며 `/tmp` 사용량은 0 B였다. 이번에도 OOM을 지지하는
증거가 있고, disk/inode 고갈 증거는 없다.

**전체 성능 개선으로 판정하지 않는다.** 후보 peak는 기준선의 0.9984708794배로
거의 같고, 같은 rank·piece·phase에서 더 늦게 실패했다. Ready worker의 도착 순서에
따라 piece 소비 순서와 중간 node 생성 순서는 달랐으며, 각 arm은 한 번의 표본이다.
미완료 수명을 완료시간 비율로 만들거나 이 차이를 안정적인 회귀율로 단정하지 않는다.
실제 worker owner 수명 단축과 전체 검색 성능 개선은 분리해서 기록한다.

마지막 누적 계측은 guard suffix 생략 3,385,579회, filter misses 97,329,815회,
guard misses 494,678,208회, Union remainder 1,924,132회였다. Remainder는 누적
27,805,713 nodes를 만들었고 ZDD work 148,270,376을 사용했다. 이것은 현재 live
node 수와 다른 누적량이다. 전체 Down work는 1,913,126,912, ZDD work는
2,749,890,560, Boolean work는 2,838,773,065였다. 물리 이동의 일부 생략과
owner 조기 해제만으로 중앙 relation×geometry의 증가를 해결하지 못한 결과다.

후보는 연구용 `symbolic-family` 경로에 남으며 제품 기본 탐색의 교체 근거로
채택하지 않는다. 두 셀 실행 모두 accepted 결과·CTK4 export에 도달하지 않아
Full P7P4 identity/global-source 대조나 full joint 검증을 완료했다고 말할 수 없다.

## 7. 원시 증거

원시 자료는 `_local/artifacts`에 보존하고 이 문서에는 검토한 요약만 담는다.

- `p7p4-legacy-clean-baseline-20261124.{log,summary.json,runtime.json,analysis.json,kernel-time.json,before.txt,after.txt}`
- `p7p4-legacy-clean-baseline-20261124.identities.ctk1`, `.coverage.u64le`
- `p7p4-cells-clean-baseline-20261124.{log,runtime.json,analysis.json,kernel-time.json,before.txt,after.txt}`
- `p7p4-baseline-pair-20261124.json`, `measure_comparison.rs`
- `p7p4-comparison-baseline-build-20261124.log`, `symbolic-family-compare-baseline-20261124.sha256`
- `p7p4-cells-candidate-20261124.{log,runtime.json,analysis.json,kernel-time.json,before.txt,after.txt}`
- `p7p4-cells-source-pair-20261124.json`, `p7p4-legacy-candidate-pair-20261124.json`
- `p7p4-candidate-tests-review-20261124.json`, 세 개의 `p7p4-candidate-*-tests-20261124.log`
- `p7p4-comparison-candidate-build-20261124.log`, `symbolic-family-compare-candidate-20261124.sha256`
- `p7p4-{baseline,candidate}-build-timings-20261124.json`

Baseline release build는 두 비교 binary를 `--timings`로 빌드해 Cargo 표시
3분 12초에 완료했다. Toolchain/cache 복구 직후의 cold build로, 이전 환경이나
후속 warm build와 직접 compiler speedup을 주장하지 않는다. Release LTO와
codegen 설정을 줄여 solver 실행 성능을 희생하지 않았다.

CI 조회·실행·대기, main 승격, PR·release·deploy는 이번 작업의 범위가 아니다.
Full4의 완전한 정합성과 60초 미만 완료가 확보되지 않아 Full6를 실행하지 않는다.

## 8. 남은 구현 우선순위

- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/geometry_compatibility_filter.rs`, 구현 내용: 연속 Append guard를 하나의 restriction으로 합치거나 이미 계산된 오른쪽 Union 결과를 재사용하여 실제 중간 family 생성 자체를 줄이고, memo 재사용 손실과 새 조회 비용을 함께 검증한다.
- [ ] 수정 파일: `crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_family.rs`, `crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs`, 구현 내용: 기존 정적 certificate로 제거할 수 있는 순서·물리 제약을 DAG의 정확한 owner와 조건에 연결하고, 후보 전수 prepass나 다른 폭증하는 관계를 먼저 만들지 않는 경계를 구현·계측한다.
- [ ] 수정 파일: `crates/clearra-core-executor/examples/symbolic_family_compare.rs`, 구현 내용: 완료된 셀 결과를 확보한 뒤 저장 CTK4의 verified membership·정확한 cardinality로 legacy identity 집합을 stream 검산하고, 원래 source bitmap 및 full joint 검증 범위를 분리한 후처리 증거를 남긴다.
