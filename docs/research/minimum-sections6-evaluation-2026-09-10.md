# Minimum 후보 6.1–6.3 전체 평가

작업 기준은 `minimum-parallel-overhead-and-algorithm-plan-2026-09-08.md`의
6.1–6.3 전체다. 기존 H/O 결과의 후속이며, 서로 대체 관계인 가설은 독립
실험으로 비교한다. 이 문서는 로컬 알고리즘 실험이며 릴리스/CI 승인 자료가 아니다.

- 소스 브랜치: `codex/v0.8.0-hotfix-minimum-algorithm-ab`.
- 기준 커밋: `a2cdb2811e76dc0cc3158c2a00c2b3783937bf4b` (H/O/M2 기본 적용).
- 입력: 최소 해법, P7, `ctk3_w0kCQBjwwAMPPAD37g`, 4L,
  Jstris180, empty hold. 실제 logical 12, compute 4/8/11/12를 별도 측정.
- 원본 행렬: `63de33e1d86077c179a38f6311df893ba9abcc13c9b18fa433384f5961eeee91`.
- 4195의 비공개 A/B GUI를 사용한다. 일반 4194 제품 GUI를 복제하지 않는다.
- 원본 ID의 첫 canonical 집합, 정확한 K, 5,040개 큐 커버,
  후속 동률 해의 lazy 열거를 보존한다. 알려진 25를 제품 입력/힌트로 넣지 않는다.

이번 브랜치의 일반 빌드에는 **T+F**와 worker pool 종료 순서 수정을 적용한다.
N은 11 worker에서는 빨랐지만 4 worker에서 느려지고 편차가 커져 독립 실험으로
남긴다. worker 수의 전역 기본 정책은 바꾸지 않는다. T+F도 모든 환경의 성능
우위를 입증한 것은 아니며, 아래의 4 worker 약 1.9% 회귀를 포함해 판단해야 한다.
첫 canonical 표시의 3~5초 목표는 달성하지 못했다.

## 이번 구현

### T — distinct-row dual capacity (B1b)

기존 root dual의 `k * D` 상한을 남아 있는 서로 다른 행들의 실제 기여 합으로
강화한다. 현재 미커버 제약의 비음수 정수 가중치 합을 N, 사용 가능한 각 행의
미커버 가중치 기여를 L(r)라고 하면, k개 행이 커버할 수 있는 가중치는
`sum(top-k L)` 이하다. 이 합이 N보다 작으면 현재 분기는 불가능하다.
또한 각 선택 행은 `N - sum(top-(k-1) L)` 이상의 기여가 필요하므로 기존의
conditional-row 필터에도 더 강한 필요조건을 전달한다.

현재 incidence, covered, selected, excluded를 다시 읽어 checked-u128로 계산한다.
기존 root denominator를 새로운 제약의 검산 없이 재사용하지 않는다. overflow나
부적합한 크기는 가속을 생략한다. 최대 256행의 고정 scratch만 사용하며, 새로운
부동소수점 권한이나 query 간 mutable state를 추가하지 않는다.

### F — exact 1/2-row canonical tail (F1의 작은 커널)

마지막 1~2행을 정하는 canonical 단계에서는 원본 행렬과 검증된 witness를 유지한
채 원본 ID 순서대로 직접 커버를 판정한다. 새 residual matrix/selector/worker
질의를 만들지 않는다. 한 조합을 한 work unit으로 세고 cursor를 저장하며,
취소·clone·메모리 회계를 기존 paging 계약에 연결한다. 첫 유효 조합만 반환한다.
직접 검사하는 suffix는 최대 256행으로 제한하고 더 큰 suffix는 기존 exact
oracle을 사용한다. 전체 family count, ZDD 또는 모든 동률 해 materialization을 추가하지 않는다.

### N — assistance 중복 작업 대조 (O3)

원래 disjoint root 작업을 모두 유지하면서 idle assistance 발행만 끄는 독립 대조다.
이 설정은 receipt를 만들거나 원래 proof 의무를 제거하지 않는다. 기존 assistance는
부모 cursor와 자식 cube를 경주시킨다. 실제 미방문 DFS subtree 이전(O4)과 다르다.

N의 최초 두 번은 약 5초에 `distributed verifier pool cancelled`로 실패했다.
원격 작업을 마친 `completeAtomicTasks()`가 pool을 닫은 뒤 남은 warm callback이
동일 query의 redundant-sibling 취소를 요청하는 실제 종료 순서 경쟁이었다.
모든 task issuer와 warm 작업을 join한 뒤 단 한 번 drain/close하도록 호스트를
수정했다. 원래 proof 검산과 generation 검사를 완화하지 않았다. 실패 표본은
시간 비교에서 제외하고, 수정 이후의 동일 호스트 코드로 batch를 다시 측정한다.
기존 worker-pool/runner 계약과 이 종료 순서를 재현하는 계약이 통과했다.

### B1a/B1a-2/B2 실험 solver

로컬 scout에 OR watched propagation, native cardinality의 이유 clause, first-UIP
학습, activity/restart, 동일 matrix/fixed-K의 assumption 재사용을 구현했다.
비교 encoding은 native PB와 sequential CNF다. assumptions를 영구 unit으로
바꾸지 않는다. native PB의 K를 단조 감소시키면 이전의 더 약한 K에서 도출한
학습을 유지할 수 있으며, K 증가나 matrix 변경은 새 인스턴스에서 시작한다.
원본 246개 ID의 canonical 질의에서는 dominance 축약 행렬을 사용하지 않는다.

이 scout는 JS 실험 코드이며 portable Rust 제품 backend로 연결되지 않는다.
시간/논리적 clause 용량 한계에 도달하면 UNKNOWN을 반환한다. 정확한 제품 전체
메모리 admission, cooperative WASM ABI, proof receipt 권한은 별도 구현 대상이다.

추가 대조에서는 native PB 전파의 이유를 영구 watched clause로 저장하는 대신
assignment와 함께 되돌리는 이유 snapshot으로 보관했다. first-UIP의 학습 결과만
영구 저장한다. 기존 eager 방식과 lazy 방식 모두 남겼다. 논리적 clause/literal
용량 제한은 유지하지만, 이 제한은 JS 전체 메모리 admission을 구현한 것이 아니다.

## 정확성 검증

- 컴파일된 Rust/WASM solver: 4행/3제약 전체 행렬의 정책 5개,
  limit 0–4에 대한 102,400개 결정과 모든 유효 해의 partition 소유권을 전수 대조.
- 63/64 경계와 130번째 bit에 걸친 제약으로 16,875개 coverable 행렬의 모든
  최적 동률 해를 원본 ID 순서와 비교. page size 1 / work budget 1.
- 실제 Rust dual 메서드를 포함한 로컬 WASM probe: 786,432개 가중치·미커버·제외·K
  조합에서 pruning과 conditional-row 필요조건을 모든 유효 부분집합과 대조.
- Scout: 28,672개 작은 고정-K/assumption 질의와 7,200개 8변수 무작위 질의에서
  전수 열거와 일치. 후자는 실제 3,025개 conflict 분석 경로를 실행했다.
  별도로 8,192개 행렬에서 K의 단조 감소와 학습 재사용을 전수 열거와 대조했다.
- 최종 일반 설정(실험 feature 없음): 20,480개 결정, 3,375개 동률 행렬,
  786,432개 dual 검산. transactional clone, 취소, 메모리 거절 후 재시작도 대조.
- Lazy PB 이유: 28,672개 작은 질의, 7,200개 8변수 질의(3,276 conflicts),
  743개 단조 감소 질의 통과. eager도 같은 743개 추가 감소 질의를 통과했다.
- 위 검증은 로컬 수학/구현 검증이다. 새 CI 게이트나 native 제품 실행은 추가하지 않았다.

## 4195 실제 브라우저 batch 결과

장비는 Intel Core 5 210H, 8 physical / 12 logical, 약 15.6 GiB RAM,
Windows 10.0.26200 x64다. raw report에는 실제 브라우저 UA를 남겼다.
한 표본은 fresh workers를 소유하고, batch는 3회씩 순차 실행했다. prewarm/import는
별도 기록하며, 표본 사이 OS·브라우저·WASM code cache는 남을 수 있다.
아래 시간은 prewarm 후 명령 시작부터 실제 ProductResultPager의 25개 field를
그린 뒤 두 animation frame까지의 시간이다. CPU 사용률, affinity, physical-core
배치, 전력 모드, 열 throttling은 계측하지 않았으며 idle worker 수로 추정하지 않는다.

처음 4개 성공 표본은 종료 순서 수정 전이므로 비교에서 제외했다. 수정 후 P7
성공 표본 48개는 같은 matrix, canonical members hash, 5,040큐 커버,
`known_alternative_count=1`, 전체 개수 미확정, lazy enumeration을 검산했다.
최초 N 2회는 실제 pool 종료 경쟁으로 실패했고, 최초 12 worker 1회는
`--use-all-cpu-threads` 누락으로 명령이 거절됐다. 이 3개는 시간 성과로 세지 않았다.

### 같은 11 worker에서 알고리즘 비교

| 정책 | 성공 n | 중앙값(초) | 최소–최대(초) | 판단 |
| --- | ---: | ---: | ---: | --- |
| HO 기준 | 6 | 15.833 | 15.347–16.089 | M2/H/O 유지 |
| T | 3 | 15.473 | 14.937–15.554 | 강한 정수 capacity 검사 |
| F | 3 | 16.114 | 15.691–16.335 | 단독 wall-time 이득은 확인 안 됨 |
| N | 3 | 15.157 | 15.020–15.478 | speculative assistance off |
| T+F | 6 | 14.924 | 14.595–15.847 | 기준 대비 약 5.7% 감소; 일반 설정 채택 |
| T+N+F | 6 | 14.046 | 13.675–14.341 | 약 11.3% 감소; 장비별 실험으로 유지 |

HO와 TF는 후반에 각각 batch 3회를 추가해 반복했다. 이 표본 수에서 통계적
유의성이나 다른 CPU에서의 우위를 주장하지 않는다. F는 일부 작은 canonical
질의를 없애지만 전체 첫 응답 시간을 단독으로 개선했다고 해석하지 않는다.
증명 witness와 trivial prefix에 따라 질의 수가 달라져 모든 실행에서 고정된
개수의 wave가 줄어든다고도 주장하지 않는다.

### worker 수와 구현을 함께 비교

| 요청 compute workers | HO 중앙값(초) | TF 중앙값(초) | TNF 중앙값(초) | TNF 범위(초) |
| ---: | ---: | ---: | ---: | ---: |
| 4 | 40.732 | 41.507 | 42.555 | 42.266–63.100 |
| 8 | 17.358 | 미측정 | 16.641 | 15.994–16.715 |
| 11 | 15.833 | 14.924 | 14.046 | 13.675–14.341 |
| 12 | 20.518 | 미측정 | 18.517 | 18.234–18.965 |

11의 표본 수는 위 표와 같고 나머지는 각 3회다. 실제 logical 값은 모두 12로
유지했다. 4/8/11은 별도 제어 worker와 요청 개수의 remote compute workers를
사용한다. 12는 명시적 `--use-all-cpu-threads`를 사용해 remote 11개와 계산에도
참여하는 관리 worker 1개가 된다. partition 요청 수도 달라진다. 따라서 12의
지연을 순수한 context switch나 cache 문제 하나로 분해한 측정은 아니다.

이 PC와 입력에서 TNF의 12 worker 중앙값은 11보다 약 31.8% 느렸다.
4 worker는 작업 개수 자체는 적어도 일부 증명 질의가 오래 남았다. 작은 worker
수를 다른 사양의 PC로 간주하거나, 이 결과로 모든 PC를 11 worker로 고정하면 안 된다.

V1 metadata로 본 첫 canonical query 발행까지의 중앙값은 HO 4.998초,
TF 4.763초, TNF 4.555초였다. 이후 표시까지는 각각 10.754 / 10.171 / 9.447초다.
첫 구간은 geometry·drain·K proof·질의 준비를 포함하므로 순수 K solve time이 아니다.
각 구간의 중앙값을 합산한 값을 전체 중앙값으로 사용하지 않는다. canonical 쪽의
큰 비용이 여전히 남으며 worker 조정만으로 3초가 되는 결과는 아니다.

### 다른 PC에서 재현하는 구성

`_local/benchmark-portable/minimum-sections6-20260910/`에 GUI, 동일한 AB/일반/기준
WASM과 localhost 전용 Node 서버를 묶는다. repository나 Rust build 없이 해당
PC에서 `node serve.mjs`를 실행하고 4195를 연다. 서버는 45분 lease이며 사용 중인
포트를 점유하지 않는다. 외부 서비스로 결과를 보내지 않는다.

- 실제 `navigator.hardwareConcurrency`로 초기화하고 batch 기본값은 3이다.
  처음의 logical−1 요청은 시작점이며 최적 worker 수 추천이 아니다.
- 각 PC에서 지원되는 1/2/4/8/logical−1/logical을 비교한다. 이 GUI는 표본당
  최대 32 workers를 허용한다. 예약 logical processor 사용은 명시적 CLI 옵션이다.
- CPU model·OS·architecture·RAM·Node 버전은 해당 서버가 기록하고, 브라우저 UA·
  logical·device-memory 보고값 및 실제 admission snapshot은 별도로 기록한다.
  전력 모드와 다른 부하는 Run notes에 남길 수 있다.
- 배포한 묶음의 서버는 시작마다 임의의 measurement session ID를 부여한다.
  사양 보고값이 같은 서로 다른 PC나 별도 측정 세션도 자동으로 합쳐지지 않는다.
- 같은 장비·브라우저·WASM·명령·worker·전력/부하 조건의 표본만 같은 군으로 묶는다.
  조건이 달라지면 별도 군으로 비교하며 다른 PC 속도는 그 PC의 실제 결과가 있어야 한다.
- 각 artifact의 byte length/SHA-256을 실행 전에 검산한다. GUI의 build-time source와
  실제 정적 asset 해시도 bundle에 포함한다. `results/`의 원시 JSON을 보존한다.

현재 다른 물리 PC에서의 실측은 없다. 이 묶음은 동일 조건의 후속 계측을 위한 것이다.

### 일반 빌드 및 portable GUI 최종 대조

이전 표는 메모리 보고값을 8 GiB로 고정한 기존 GUI 조건이다. 새 GUI는 실제
브라우저 보고값을 사용했고 이 장비의 Chrome 152는 16 GiB를 보고했다.
서로 다른 admission/GUI 조건을 같은 군에 합치지 않는다.

일반 빌드 첫 batch는 14.976 / 19.574 / 32.805초로 크게 흔들렸다. 이어서 같은
새 GUI의 실험 TF도 19.271 / 16.141 / 15.332초로 변했다. 이 편차의 원인을
특정 CPU/열/캐시/브라우저 스케줄링 원인 하나로 확정하지 않았으며 원시 표본을
제거하지 않았다. 이때는 visibility를 기록하지 않아 그 영향도 소급 판정하지 않는다.

표본 시작부터 실제 paint까지 visibility 변화를 기록한 최종 GUI에서 TF→D 순서로
각 batch 3회를 실행했다. 모두 actual logical 12 / compute 11 / reported memory
16 GiB이며, compiler 또는 다른 solver를 동시에 실행하지 않았다.

| 동일 최종 GUI의 대조 | 각 표본(초) | 중앙값(초) | 탭 상태 |
| --- | --- | ---: | --- |
| 실험 TF | 16.446 / 16.324 / 16.738 | 16.446 | 3회 모두 계속 visible |
| 일반 D (T+F) | 14.708 / 15.347 / 15.671 | 15.347 | 3회 모두 계속 visible |

최종 ordinary binary에서 실험용 정책 setter export가 없는 것을 확인했다.
portable P7 12회 모두 같은 원본 canonical members와 5,040큐 커버를 검산했다.
최종 결과도 3~5초 목표를 충족하지 않는다. 이전 실험의 5.7% 감소를 새 환경군의
보장된 개선율로 사용하지 않는다. 단일 장비의 작은 batch에서 일반적 통계적
성능 우위를 확정하지 않는다.

탭이 숨겨지면 그 표본을 별도 군으로 남기며 계산 결과를 실패로 바꾸지 않는다.
180초 fixture 제한은 결과 수신뿐 아니라 paint까지 포함하므로 숨겨진 탭의 rAF가
표본 종료를 무기한 막지 않는다. 이 설정은 비공개 benchmark에만 적용된다.
`node analyze.mjs`는 artifact·GUI·실제 장비·브라우저·admitted memory·notes·visibility가
다른 자료를 서로 다른 군으로 집계하고 원본 canonical hash를 확인한다.

## Q1–Q4 reference와 B1a/B2 scout 결과

Qnia `03b637730c5b541f4f2934be613498fbe65327fd`의 같은 행렬과 CP-SAT WASM을
사용했다. 브라우저 실험과 동시에 실행하지 않았다. 아래 API 시간은 이미 만든
model을 solve하는 구간이다. import 26–32ms, model 구성 12–15ms는 별도 기록했다.
18회 모두 reference가 OPTIMAL 25를 반환했고 선택 행을 원본 큐 전체에 재검산했다.
이 reference 결과는 제품 proof receipt나 첫 original-ID canonical 증명을 대신하지 않는다.

| 대조 | n | API 중앙값(초) | 범위/해석 |
| --- | ---: | ---: | --- |
| full1 / workers1 | 2 | 5.307 | 5.289–5.325 |
| full1 / workers2 | 2 | 4.990 | 4.858–5.123 |
| full1 / workers4 | 2 | 4.858 | 4.806–4.910 |
| full1 / workers8 | 2 | 7.120 | 7.033–7.207 |
| full1 / workers11 | 2 | 8.100 | 7.437–8.762 |
| diverse4 / seed1 | 2 | 5.635 | 5.563–5.706 |
| diverse4 / seed2 | 2 | 6.249 | 4.050–8.448; 편차 큼 |
| full1 / workers2 + greedy hint | 2 | 3.656 | 3.596–3.716; 입력 유래 30행 |
| full1 / workers2 + clause sharing off | 2 | 4.754 | 4.395–5.112; 단독 채택 근거 부족 |

Q1은 workers를 1→2→4→8→11→11→8→4→2→1 순서로 실행했다.
full solver 1개를 고정해도 log에는 worker 증가에 따라 first-solution 및
interleaved solver가 늘었다. 따라서 고정 full solver 수를 동일한 총 탐색량이나
pure CPU scaling으로 해석하면 안 된다. Q3의 약 26.7% 이득은 유망하지만 해당
reference의 K proof 성과이며 Clearra GUI의 canonical 표시 시간과 직접 비교하지 않는다.

Scout의 eager native PB는 K=25와 K=24 모두 0.27–0.38초 만에 20만 clauses에
도달해 UNKNOWN이었다. 이는 빠른 증명이 아니라 표현 비용에 의한 실패다.
이유를 assignment 수명으로 제한한 lazy 대조에서는 10초 동안 약 7.3만/7.7만
conflicts를 처리하고 영구 clauses 약 7.5만/7.9만, 이유 snapshot peak 약 3.5천
literals를 유지했지만 두 질의 모두 시간 한도 UNKNOWN이었다. Sequential CNF도
10초 안에 두 질의를 끝내지 못했다.

원본 246-ID의 첫 canonical assumption은 warm/cold 모두 lazy에서 5초 한도로
UNKNOWN이었다. 따라서 학습 재사용으로 전체 canonical이 빨라졌다고 주장하지 않는다.
입력에서 얻은 witness의 끝 2행 앞 prefix를 조건으로 준 6개 작은 질의는 답이 일치했고,
첫 질의 이후 warm 0.23–0.99ms, cold 0.91–1.03ms였다. 이 prefix 자체는 첫 canonical로
증명되지 않았다. Greedy 30에서 K를 낮추는 lazy 시도도 첫 K=29에서 5초 UNKNOWN이었다.
표현 비용 개선은 남기되 B1a/B2의 제품 backend 교체는 채택하지 않는다.
이 JS scout의 결과로 해당 알고리즘 계열이나 향후 Rust 구현의 가능성을 배제하지 않는다.

## 구조 검사

독립적으로 검산한 같은 원본 행렬은 246개 행, 중복을 제거한 2,407개 제약이다.
Qnia의 proof-only kernel은 158개 행, 1,389개 제약, forced 0개다.
원본과 이 kernel 모두 연결 성분은 1개다. kernel의 binary 제약은 0개다.
따라서 단순 component optimum 합산과 binary graph의 odd-cycle bound를
이번 P7의 직접적인 개선으로 채택할 근거가 없다. 조건부 분해/더 일반적인 cut의
가능성까지 배제한 결과는 아니다. Qnia kernel과 Clearra 자체 kernel의 수를 섞지 않는다.

## 6.1–6.3의 각 항목에 대한 판단

| ID | 이번 처리와 제품 경계 |
| --- | --- |
| V0 | 이전 a2cdb28에서 serial profiling 소유권 충돌 수정. 이번 baseline에 포함. |
| V1 | 비공개 GUI에서 query의 matrix/generation/K/행·제약 수와 시점을 추가 수집. 기존 wave interval과 대조하며 nested worker 시간을 wall time에 더하지 않음. 일반 wire에 purpose 권한을 추가하지 않음. |
| O1 | 기존 H/O baseline 유지. 같은 튜닝을 새 알고리즘으로 중복 계상하지 않음. |
| O2 | 새 matrix 전송/준비 비용은 기존 profile의 수십 ms 범위. F의 작은 suffix에서 직접 회피. 일반 immutable matrix+delta protocol은 아직 제품 미구현. |
| O3 | N 대조 구현·측정. 원래 proof 작업의 완전성 보존. |
| O4 | 현재 구현은 racing임을 확인. subtree donation은 cursor/checkpoint와 receipt 소유권 이전까지 필요한 별도 프로토콜이며 이번 경로에 섞지 않음. |
| O5 | 현재 residual warm은 동일 solver 내부. shard 간 공유에는 원본 pattern remap과 independent recertification이 필요. 이번 T는 현재 incidence 재검산만 사용. |
| O6 | 기존 double-encode/yield 후보는 별도 유지. 이번 알고리즘 효과와 섞어 수치화하지 않음. |
| H | 기준 커밋에 적용 완료. |
| B1a | 실제 이유/학습/undo/restart를 갖춘 scout로 P7 평가. 제품 전파/학습 엔진 교체는 별도 채택 판단. |
| B1a-2 | native PB와 sequential CNF 독립 비교. 이전 totalizer/network 계열 결과와 동일 실험이라고 주장하지 않음. |
| B1a-3 | scout의 activity/restart 포함. conflict minimization·core-guided/MaxSAT 전체를 구현한 것은 아님. 기존 core-only 회귀 보존. |
| B1b | T 구현·검산·A/B. 새 LP solver 또는 모든 정수 cut을 도입한 것은 아님. |
| B1b-2 | component/binary incidence 구조 검사. 기존 G 회귀 유지. 일반 hypergraph/CG cut의 가능성은 남음. |
| B2 | fixed-K assumption 학습 재사용 scout에서 warm/cold 동일 질의 비교. 제품 canonical ID/lazy 계약 유지. |
| B2-2 | native PB의 K 단조 감소와 학습 유지 구현. 증가/새 matrix는 fresh instance. 입력에서 만든 greedy 상한부터 내려가는 대조로 평가. |
| B2-3 | 임의 정수 lex weight를 도입하지 않음. K 증명 이후 원본 ID assumption self-reduction으로 평가. |
| B3 | 기존 proof-only kernel 유지. 원본/kernel 단일 성분 확인. |
| B3-2 | 단순 component 분해 결과를 separator/대칭의 불가능성으로 확대하지 않음. 원본 ID 복원과 결합 증명이 없는 축약은 적용하지 않음. |
| B4 | 기존 검산 witness 공유 보존. Q2/Q4에서 portfolio/공유 설정 비교. scout 학습은 동일 matrix 및 기존 K 이하의 논리 범위에 제한. |
| U | 입력에서 계산한 greedy 30행 witness로 Q3 대조. 알려진 최적 25 주입 없음. |
| F1 | 1/2-slot kernel인 F 구현·측정. ZDD, rank/unrank, component product, 전체 family count는 별도 남음. |
| Q1 | full subsolver 수 1을 고정해 workers 1/2/4/8/11 역순 반복. 실제 solver log의 구성도 보존. |
| Q2 | max_lp/default_lp/quick_restart/no_lp 4종과 seed 1/2 비교. worker 수와 실제 전략 수를 별도로 기록. |
| Q3 | greedy 입력 유래 hint와 fresh K proof 이후 decision을 구분. startup/model/solve 시간 구분. |
| Q4 | binary/glue clause 공유 off 독립 대조. 이전 interleave 회귀를 재실행 없이 보존. shared-tree 전체를 검증했다고 주장하지 않음. |
| Q5 | 같은 행렬 CP-SAT와 자체 scout의 K/negative/canonical endpoint 구분. 기존 성공한 HiGHS 비교는 역사 자료로만 유지. |
| E1 | 외부 CP-SAT/HiGHS 제품 adapter 미도입. 현재 core ABI에 동일한 메모리·취소·capability·재현 빌드/NOTICE를 제공하지 못한 실험을 제품 증명으로 승격하지 않음. |
| L1 | geometry 전용 별도 후보 유지. 누락된 legal-board index를 UNSAT/invalid의 근거로 사용하지 않음. |

## 재현 도구

실험 WASM의 source SHA-256은
`11011946ff7031831e216a94765876ce48e8efeae5ae2579018219fa0abe6fc4`,
binary SHA-256은 `8e57fc0371b92ce9e7798a031592defc668bf2264a6d254a3dea2ea69fee06d4`다.
수정 후 batch에서 실제 실행한 worker bundle의 SHA-256은
`3875a9e38d050ced03e189020b90f6cbf4dd39d80e60bea0e78b403f3fe36a0d`이며,
파일은 `clearraWorker-C6UgA_8z.js`다. 일반 빌드는 실험 ABI를 포함하지 않고
T/F 기본값, suffix 상한, issuer에서의 N 정책 분리를 다시 컴파일한 별도 artifact다.
일반 빌드 source는 `87af6c7729c8434887b31dbdf5d0ad659925f76663324faee620357ba5bc9ce9`,
WASM은 `10d55fd672404da1dadff2dd9464105badc5f30158744d8139e886645ad85bb1`이다.
배포용 source/engine commit authority는 부여하지 않은 로컬 build다.

raw 47–55의 `harness_sha256`은 source 편집 이후 수집 시점의 hash이며,
실행 중인 정적 GUI의 hash가 아니다. 이 값으로 실행 코드를 판정하지 않는다.
실제 사용한 static assets는 `minimum-sections6-measured-gui-20260910.json`에
기록했다. 후속 portable bundle에서는 build 시점의 source/asset 해시를 고정해
수집 시점의 source 파일 상태와 혼동하지 않도록 했다. 원시 결과 파일은 수정하지 않았다.

원래 checkout의 `_local/reports/` 아래 원시·집계 자료:

- `minimum-sections6-browser-20260910/` (55회: 성공 52, 실패/거절 3;
  수정 전 성공 4회는 이후 비교에서 제외).
- `minimum-sections6-summary-20260910.json`, `minimum-sections6-stats-20260910.json`.
- `minimum-sections6-portable-summary-20260910.json`,
  `minimum-sections6-portable-groups-20260910.json`.
  portable 원시 자료는 `_local/benchmark-portable/minimum-sections6-20260910/results/`에 있다.
- `minimum-sections6-properties-20260910.json`,
  `minimum-sections6-properties-ordinary-20260910.json`.
- `minimum-sections6-reference-20260910.json`,
  `minimum-sections6-reference-lazy-20260910.json`.

원래 checkout의 `_local/research/` 아래:

- `build-sections6-candidate-20260910.mjs`
- `build-sections6-default-20260910.mjs`
- `minimum-sections6-server-20260910.mjs`
- `minimum-sections6-reference-20260910.mjs`
- `minimum-incremental-pb-scout-20260910.mjs`
- `verify-incremental-pb-scout-20260910.mjs`
- `run-sections6-properties-20260910.mjs`
- `export-minimum-benchmark-20260910.mjs`
- `portable-minimum-benchmark-server-20260910.mjs`

원래 checkout은 `C:/Users/강민수/Desktop/프로젝트/Clearra/Clearra`이며, 제품 소스는
`C:/Users/강민수/AppData/Local/Clearra/worktrees/v080-hotfix-minimum-algorithm-ab`다.
외부 source/runtime은 별도 pinned reference checkout에 남겨 두며 CI나 배포에 포함하지 않는다.

설정 의미는 [OR-Tools v9.15의 공식 parameter 정의](https://github.com/google/or-tools/blob/v9.15/ortools/sat/sat_parameters.proto#L625-L710)를
확인했다. full subsolver 수와 전체 worker 수는 별개이며, 공유/interleave의 의미를
실제 실행 log와 함께 해석한다.
