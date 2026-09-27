# v0.8.1 가속기·워커 소유권·제품 수명 수정 — 2026-09-27

## 범위와 권위

사용자 지시로 PC4 Tablebase/v0.9.0 업그레이드는 동결한다. 기존 TB 소스와
증거는 보존하며 이번 검증에 TB 실행·다운로드·신규 qualification을 포함하지 않는다.

- 제품 main 확인 기준: `521fee125b478a5a39a90a808d6c471c430c8494`.
- 구현 기준: `codex/converge-v081-v090-release-20260923`,
  `170026643db28b82b33a12bece5496e65c3bb947` 위의 미커밋 변경.
- 두 브랜치의 혼합 변경 전체를 main으로 병합하지 않는다. main의 Build drawing,
  mandatory/pinned, browser recovery, command registry 수정은 선별 통합 때 보존한다.
- 아래 테스트는 로컬 경계 증거다. 앱 qualification, exact-SHA release acceptance,
  실제 CLI/Web/Desktop/Discord readback 또는 배포 완료를 뜻하지 않는다.
- 변경하지 않은 다섯 profile 자산을 다시 생성하거나 전수 자격 검증하지 않는다.

## 가속기 계산 경로

### Legal-board: 거부 위치와 중복 조회

기존 경로는 child의 도달성 계산과 edge/node 작업을 한 뒤 dequeued node에서
음성 필터를 적용했다. child subset의 qualified negative를 비싼 도달성 전에
검사하고, 기존 generation/subset node-ID 저장소에 pending/rejected 상태를 둔다.
새 worker별 전체 table은 추가하지 않는다. pending positive는 reachable 증거가
아니며 실제 성공한 edge만 node-ID로 승격한다. 원래 행 대응, profile/rule,
empty-origin 10×4 exact 4L 범위의 권위는 그대로다.

새 prune counter는 도달성 전에 거부한 child subset의 최초 횟수다. 과거의
dead-node counter와 수치를 직접 비교하지 않는다.

### 조건부 도달성: Boolean target의 합성 방향

현재 qualified pack의 닫힌 sky-window 관계를 적용한 뒤 광범위한 forward BFS로
목표를 찾던 경로를 개선했다. Boolean CountUnique이고 witness·spin·finesse·별도
실행 제약이 필요 없는 cold query에 한해 proven exit를 기존 exact reverse 탐색의
추가 success seed로 사용한다. 기존 sky seed를 유지하며, 막힌 출구나 domain 밖
질의는 기존 정확 경로다. first-success ordered kick 계약은 바꾸지 않는다.

CountAll·전체 lock family·witness 등 더 강한 증거 모드는 원래 composer를 쓴다.
bitmap은 최대 168바이트 stack 자료이며 새 heap table은 없다. reference/off 경로는
const-generic 분리로 proven-exit 판정을 수행하지 않는다. hit는 완료된 Boolean
질의를 포함하며 전체 lock-family 증명 횟수가 아니다.

## 워커 메모리 소유권 판정

| 자료 | 판정 | 구현 또는 남은 조건 |
| --- | --- | --- |
| SearchProblem, GeometryCatalog, target group, candidate family, qualified asset | 읽기 전용 공유 유지 | 기존 Arc owner를 재사용; worker별 자산 복제하지 않음 |
| StandardBag cursor 전이·suffix count | 요청별 공유로 이동 | `Arc<SharedStandardBagRequest>`와 lazy OnceLock. 전체 supply/hold/provenance와 universe identity 결박 |
| 최초 aggregate·최초 coverage row | 불필요한 복사 제거 | 최초 결과 owner를 이동; 이미 Arc인 PatternBitSet을 dense zero 생성 없이 채택 |
| geometry frontier, stack, projection/graph mutable arrays | worker별 유지 | 변경 가능한 실행 상태를 전역 lock memo로 이동하지 않음 |
| StandardBag decision nodes, language ID/interner, coverage masks/queues | worker별 유지 | ID/recycle epoch와 소유 workspace에 종속; 공유 시 의미·경합 검증이 별도로 필요 |
| Reachability cache, visited, BFS/reverse scratch | worker별 유지 | cache 논리 payload 약 5,505,024바이트/worker; mutable cache의 전역 공유는 채택하지 않음 |
| Reachability template·sky entry의 immutable 부분 | Open | 현재 lazy mutable Option을 요청-bound owner로 바꾸는 추가 설계 필요 |

공유한 supply table은 P7/P7P4에서 수십 KiB 수준이다. 이것만으로 큰 private memo의
worker 수에 비례하는 전체 RSS를 해결했다고 주장하지 않는다. duplicate coverage
union은 여전히 필요한 COW를 수행한다. 기존 compact memo 후보는 기본값으로
승격하지 않고 reference 기본값을 유지한다.

영수증의 `worker_retained_bytes`는 종료 시 private payload 합계이며 OS peak가 아니다.
`shared_standard_bag_request_retained_bytes`는 요청 owner 한 번만 계산한다.
`standard_bag_memo_payload_bytes`는 private StandardBag의 부분량이므로 다시 더하지
않는다. 하네스는 합계 불일치와 이전 혼합 scope의 영수증 결합을 거절한다.
Windows sampled process-tree working set, 감독 aggregate commit peak,
active accelerator shared peak 128MiB gate는 서로 다른 계측이다.

## 제품 수명·오류 수정

- Web 명시 다운로드의 상태/크기/overflow/abort/observer 오류에서 body와 reader를
  취소하고 lock을 회수한다. 진행 중 `read()`도 취소로 종료한다.
- Desktop check/status/remove는 실제 blocking 작업이 끝날 때까지 RAII lease를
  유지한다. client는 check → status를 같은 선택으로 순차 실행한다.
- signed native store의 동일-generation 손상은 명시 Download로 복구할 수 있다.
  signed length를 쓰기 전에 검사하고 digest와 full qualifier를 통과한 새 immutable
  sibling을 journal로 활성화한다. 기존 owner/reader와 원래 payload는 덮어쓰지 않는다.
  journal 쓰기가 디스크에 도달했을 가능성이 있으면 검증된 sibling을 남겨 dangling
  pointer를 만들지 않는다.
- 일반 App ExecutionFailed의 기본 Unsupported 오분류만 수정했다. 명시적인
  Unsupported/backend refusal과 verified ProductBuildIdentity 등록 권위는 유지한다.

## 집중 검증

| 검증 | 결과 | 권위 한계 |
| --- | --- | --- |
| Core native lib/tests typecheck, parallel/local-search-ab/qualification-reference | 통과 | native 소스 타입 경계 |
| 새 `v081_` Core 테스트 | 10 passed | early prune, Boolean reverse macro, 독립 primitive 차분, 공유 owner/identity/COW |
| 동일 Core release test binary 직접 감독 실행 | 639 passed, 0 failed, 12 ignored; 1.92초 | 이미 자격을 마친 expensive/local-only 테스트를 재실행하지 않음 |
| 위 직접 실행의 감독 상태 | 정상 종료, tree stopped, pressure 0; aggregate commit peak 460,009,472바이트 | 테스트 process-tree의 peak; 제품/자산 peak 아님 |
| benchmark admission/accounting + memo selector + Web transfer + Desktop client 순수 Node | 31 passed | 실제 solver·브라우저·Tauri 실행 아님 |
| 표준 Web test task | typecheck 및 16개 계약 통과 | 실제 WASM 빌드·브라우저 smoke 아님 |
| Core WASM typecheck | Windows 실행 정책 os error 4551로 차단 | 동일 명령 1회 수동 재시도에도 차단; 미통과 |
| CLI store repair/error 및 Desktop native lease 테스트 | 대기 | 벤치마크와 자원 경합 없이 별도 직렬 검증 |

WASM 차단을 우회하거나 WSL을 사용하지 않았다. Cargo의 첫 Core test wrapper에는
root 종료 후 descendant 종료 확인 경고가 있었으므로 정상 감독 종료로 표시하지
않는다. 위 639개 직접 binary 실행은 별도의 정상 종료 영수증이다.

## 네이티브 11-worker ABBA

사용자 선택에 따라 SRS+ empty 4L P7P4에서 legal-board와 조건부 도달성을 별도로
ABBA한다. 같은 binary, qualified generation, 11 actual workers, 456,923개 complete
result와 identity를 검증한다. solver 후보 상한을 전달하지 않으며 각 sample timeout은
1,800초다. 두 비교와 다른 heavy build/test는 동시에 실행하지 않는다.

두 비교는 다음 실행 파일로 순차 완료했다.
`SHA-256 cad78c9965f3dbd362071d2eb616e4c15899fe80ea2b15fae359dda5cce50e92`.
두 비교 모두 `complete-local-batch`이며 8개 sample의 actual worker 11,
456,923개 result, `cts1:98ebe8726537b29f`, candidate digest 및 runtime identity가
일치했다. timeout/censor/invalid는 없었다. 자산 generation은 전후 동일하다.

| 비교 | A1 | B1 | B2 | A2 | A 평균 | B 평균 | 관측 차이 | A/B sampled working-set 최대 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| Legal early-child on/off | 133.803초 | 146.286초 | 154.502초 | 162.218초 | 148.011초 | 150.394초 | B 약 1.61% 느림 | 6.02 / 5.96GiB |
| Conditioned reverse macro on/off | 168.825초 | 169.554초 | 165.875초 | 163.442초 | 166.134초 | 167.715초 | B 약 0.95% 느림 | 5.98 / 5.77GiB |

legal의 각 B는 1,942,266,463회의 first child negative를 기록했다. conditioned B의
complete hit는 각각 114,511,316과 115,047,299회였다. 활성화 자체는 확인했으나
조회 수를 실제 생략한 탐색 비용이나 전체 lock-family 증명 수로 바꾸어 해석하지 않는다.

감독 두 lease 모두 `normal`, return 0, tree stopped였다. 다만 legal은 memory
pressure 13회, conditioned는 4회로 성능 채택에 필요한 clean 실행은 아니다.
감독 aggregate commit peak는 각각 7,265,628,160바이트와 7,116,079,104바이트다.
특히 legal A1→A2의 큰 변동을 무시하고 작은 평균 차이를 확정 회귀로 말하지 않는다.
두 비교 모두 **성능 이득 미입증**이며 sample 2개/arm으로 p95 gate를 닫지 않는다.

원본 영수증은 제품 main의 선언된 `_local/artifacts/v081-priority-20260927/`
아래 `legal-11-early-child/summary.json`, `conditioned-11-reverse-macro/summary.json`에
보존한다. 연구 문서에는 이 검토 요약만 둔다. 이전 바이너리의 영수증과 합치지 않는다.

### 큰 private 자료의 실측 분해

Legal A1의 exit accounting은 다음과 같다. 이는 동시 OS peak가 아니라 worker별
retained logical payload 합계다.

| 자료 | bytes | private 합계 비중 | 소유권 |
| --- | ---: | ---: | --- |
| StandardBag | 5,311,615,760 | 86.26% | private; 큰 개선 대상 |
| Piece language | 681,310,852 | 11.06% | private ID/interner |
| Solution identity | 99,614,720 | 1.62% | 결과 owner |
| Reachability | 63,202,612 | 1.03% | private exact cache/scratch |
| Graph projection 및 기타 | 1,734,240 | 0.03% | mutable scratch |
| Private 합계 | 6,157,478,184 | 100% | 위 exclusive 항목 합 |
| Product+union memo | 3,758,096,384 | 61.03% | **위 StandardBag에 이미 포함** |
| Shared supply | 36,272 | 별도 | 요청별 한 owner |

256MiB recycle 기준은 cache의 logical live 양이다. candidate 경계에서 clear하지만
큰 capacity가 남으므로 retained 또는 필요량의 hard cap으로 해석하지 않는다.
새 후보는 row당 정확한 `(depth, normalized bag, hold)`를 보관하고 worker-local
`language:u32 → root:u32`를 저장하는 state-major product memo다. union/BDD/reducer는
바꾸지 않으며 reference/compact 기본값은 그대로 유지한다. full language/root 값과
기존 full-key SplitMix64 입력을 보존한다. row directory는 요청의 실제 최대 source
depth로 한정하고 첫 admission에서만 할당하며 각 row는 lazy다. cache admission 실패는
계산된 exact root를 반환하고 저장만 생략한다. epoch와 logical 12B/key recycle
기준은 그대로다. 별도 product/union/row 계측과 새로운 binary의 A/B 전에는 전체
절감량·성능을 확정하지 않는다.

### ABBA 이후 선택한 다음 수정

사용자는 큰 private memo 구조와 legal 조회 순서 개선을 우선했다. 위 ABBA의 legal
조회는 instantiation/충돌/grounded/clear 일치 같은 cheap filter보다 앞선다. 후속
소스는 이미 거절된 현재-generation child만 빠르게 건너뛰고 실제 concrete edge가
존재할 때 기존 child 판정을 한 번 실행한다. expensive exact lock 탐색 전 negative는
유지한다. 기존 grounded mask 필요조건은 일반 lock에만 사용하고 GeometryOnly에는
적용하지 않는다. CountUnique 첫 harddrop/realization 순서와 CountAll multiplicity는
그대로다. concrete edge가 없는 빈 scratch의 template 준비도 생략한다.

새 selector와 product/union/directory accounting이 없는 이전 binary는 작은 비계측
P7 fixture에서 하네스가 거절한다. split payload 합·private owner 포함 관계·row와
capacity bounds를 순수 테스트로 검사했다. 새 코드의 검증은 위 ABBA 이후 별도로
기록하고 이 영수증을 새 코드의 benchmark로 재사용하지 않는다.

### 후속 소스 검증 결과

| 검증 | 결과 | 한계 |
| --- | --- | --- |
| 새 Core native lib/tests 타입 검사, parallel/local-search-ab/qualification-reference | 통과; 감독 정상·pressure 0 | 실제 차분 실행 아님 |
| 제품 Core·CLI lib/tests 타입 검사, no-default-features + wasm-cpu-runtime | 통과; 감독 정상·pressure 0 | local-search-ab 미활성 제품 구성; 실행 테스트 아님 |
| 새 Core release test executable 생성 | 4분 06초 컴파일 완료 | Windows 정책 4551로 executable 실행 전 차단 |
| 같은 Cargo 명령 수동 1회 재시도 | 4551 재발, 테스트 미실행 | 자동 반복·WSL·정책 우회 없음 |
| 순수 Node 하네스/선택/accounting/Web/Desktop 계약 | 31 passed | 실제 새 solver 실행 아님 |
| Core 전체 formatting 및 변경 CLI/Desktop 파일 narrow formatting | 통과 | 보류한 TB 파일의 formatting은 변경하지 않음 |
| TB 동결 소스 해시 | 121개 모두 동일 | TB 실행/최적화/자격 검증을 재개하지 않음 |

Core 첫 실행 차단의 wrapper는 root 종료 직후 descendant 경고도 기록했으므로
정상 감독 성공으로 바꾸어 표기하지 않는다. 첫 lease와 수동 재시도 모두 최종
`process_tree_stopped=true`였다. 새 release test 실행 차단 영수증은 각각
`1790488042777893900-36192-runtime.json`, `1790488335638127800-37096-runtime.json`이다.
새 타입 검사의 정상 영수증은 `1790487956797943800-37524-runtime.json`과
`1790488355976703500-36424-runtime.json`이다. 후속 legal/memo A/B는 미실행이다.

package-wide CLI formatting check의 차이는 동결한
`tablebase_http_range_tests.rs`의 기존 formatting에만 있었다. TB 동결을 지키기 위해
그 파일을 고치지 않고 변경한 v0.8.1 파일만 별도로 검사했다. serial/no-feature Core
타입 검사는 성공했지만 native parallel 전용 accounting의 dead-code warning이
있었다. 실제 native 제품 parallel 구성의 위 Core·CLI 검사에는 이 경고가 없었다.

## Open / 다음 완료 조건

1. 후속 concrete-edge 및 state-major 소스를 집중 검증한 뒤 새로운 binary의 별도
   ABBA에서 result·actual worker·identity·time·peak·pressure를 확인한다.
2. native store/CLI 분류/Desktop lease 집중 테스트를 실행한다.
3. latest main의 제품 수정과 v0.8.1 경계만 선별 통합한다. TB dirty 변경은 제외한다.
4. CLI legal-board의 cold OutOfScope 로드 비용을 보수적인 typed activation hint로
   줄일지 평가한다. Core의 최종 negative scope는 바꾸지 않는다. Build/Setup의
   transport DTO를 실제 완료 domain으로 오인하지 않는다.
5. 분산 Web의 조건부 pack은 현재 한 verifier만 소유한다. 모든 verifier의 효과와
   shared-owner admission/128MiB 실제 peak는 별도 Open이다.
6. BuildUp p95 20%, 전체 p95 10%, 작은 입력 p95 회귀 5% 이하, 1/2/11 worker의
   전체 canonical order·coverage와 CLI/GUI/Desktop/Discord parity는 Open이다.
7. exact app SHA acceptance, 실제 배포와 rollback/readback 전까지 v0.8.1 앱은 No-Go다.

이 문서는 소스 수정과 로컬 검증의 좁은 상태를 기록하며 전체 v0.8.1 완료 선언이 아니다.
