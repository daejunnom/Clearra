# 최소 해법 CPU / WebGPU 단계별 A/B

**단계별 A/B와 병목 복구 확인을 완료했다. 제품의 기존 CPU 정책은 유지한다.**
새 실험의 큰 배치 GPU 전송·배열 경로는 빨라졌지만 같은 작업의 CPU보다 느렸고,
저장 행렬의 전체 최소성·canonical 경로도 기존 제품 최선 경로를 넘지 못했다.
GPU는 `minimum-hotfix-ab` 안의 선택 가능한 실험으로 남긴다. 현재 Cloud Run /
Discord의 CPU 경로에 GPU 의존성을 추가하지 않는다.

요약하면, 큰 32,768-state 배치의 8-worker 중앙값은 같은 session에서 기존 GPU
158.705ms → 포장·병합·결과 변환 복구 GPU 64.145ms였고 CPU는 41.300ms였다.
대기 회수, 분기 폭, 이유 공유 등의 실제 개선과 전체 탐색의 채택 여부는 아래에
각각 기록한다. 다른 자산 / session / 제품 종점의 수치를 합치지 않는다.

## 실행 계약

- 시작 소스: `81b04356682e9253bd594eefa567e06d868945f1`, 성능 브랜치에서 작업한다.
- 입력: 최소 해법, P7, `ctk3_w0kCQBjwwAMPPAD37g`. 원본 246후보 / 5,040큐와 해시로 결속한 저장 행렬을 사용한다.
- 요청 compute workers는 **4 / 8 / 11**, GUI 반복 `batch_size`는 **3**으로 고정한다. GPU 내부 상태 배치 크기와 반복 횟수는 별개다.
- 4195의 비공개 벤치마크 GUI를 사용한다. 다른 실행과 동시에 성능을 재지 않는다.
- CPU 전용 서버가 독립적으로 최소성 / 원본-ID canonical / lazy tie를 유지해야 한다. GPU 경로는 기존 `clearra-webgpu`의 wgpu/WGSL 및 장치 선택 경로를 사용한다.
- 단계별 on/off 대조를 기본으로 한다. cut과 학습, 재사용과 분할, GPU 배치와 CPU 워커 수처럼 영향이 예상되는 조합만 추가 대조한다.
- 느림 / timeout / capacity / device failure를 즉시 후보 폐기로 처리하지 않는다. 중복 탐색, 미활용 워커, 반복 준비, 전송·전치·readback, 전파 이유 및 학습 데이터 팽창, 꼬리 작업을 먼저 조사한다.
- 미완료는 UNKNOWN이다. 기존 최선 결과를 미래 탐색의 정답 힌트로 주지 않는다. 고정 K=24는 별도의 부정 증명 실험으로 표시한다.
- 성공적인 커널 시간만으로 제품 개선을 선언하지 않는다. 저장 행렬 K 증명과 원본-ID 첫 canonical 표시를 구분하고 최종 후보는 제품 종점까지 확인한다.

## 요구 범위와 진행 상태

| 단계 | 요구 / 비교 | 현재 상태 |
| --- | --- | --- |
| S0 | 원본 행렬 / 기존 최선 정책 / 4·8·11 workers × 3의 새 기준 | 완료: 제품 표본 12개, 낮은 fanout TF 기준 분리 |
| S1 | 원본-ID RHS=2 유효 cut, cut off/on 및 실제 정수 증명 비용 | 완료: cut / 변경 큐 / 인증 dual의 단계별 A/B, root 하한 21→22 |
| S2 | SAT/PB 충돌 학습, 기존 scout 병목 복구, 필요한 encoding / MaxSAT 비교 | Boolean 학습·encoding·DB·이유 공유·K 포화 대조 완료. 일반 PB / MaxSAT는 미구현 범위로 구분 |
| S3 | 같은 원본 행렬의 K / canonical assumption 재사용 off/on | 완료: selector 범위 검증, 질의 / 분기 간 재사용 A/B. 성능 완료는 UNKNOWN |
| S4 | CPU 평탄 배열 / 32-state 비트 슬라이싱 off/on | 1,024 / 32,768 상태에서 4·8·11 × 3 완료 |
| S5 | 기존 WebGPU 경로에서 같은 전파·cut 커널 off/on; 전송 포함 비교 | 실제 Rust wgpu 456개 설정·경계 검증, 큰 배치 layout A/B 완료 |
| S6 | bounded frontier, CPU 학습·증거 연결, 중복·꼬리 작업 진단 후 비교 | 완료: 재전파 / 순환 / 작은 GPU / 양성 회수 / 분기 폭 대조와 실패 복구 확인 |
| S7 | 넓은 cut 탐색 / 묶음 BDD·완화 DD / 수치 하한 등 후속 후보를 구조 근거와 앞 단계 결과로 평가 | 구조 평가 완료. 고정 relaxation의 한계와 미계측 후보를 아래에 구분 |
| S8 | 유망 조합의 제품 canonical 검증, 결과·폐기 근거·재현 자료 정리 | 완료: 기존 제품 12개와 저장 행렬 전체 15개 성공 표본 확인. 새 구성의 제품 승격은 보류하고 재현 자료 보존 |

S4/S5의 자료구조 검증이나 커널 결과만으로 이 작업을 완료 처리하지 않는다.
부정 결과를 얻으면 해당 구성의 병목과 복구 시도 또는 구조적인 배제 근거를 남긴다.
전체 조합의 무차별 실행은 하지 않는다.

## 증거와 기준의 분리

이전 보고서의 서로 다른 WASM / session 시간은 새 대조군과 합치지 않는다.
Qnia의 기존 GUI 3~5초는 실제 계측 기준으로 유지하며 다시 측정하지 않는다.
문서의 외부 인증서 링크만으로 하한 22 / treewidth 72를 이번 실행에서 검증했다고 기록하지 않는다.
입력 또는 정책 변경은 별도 artifact identity를 남긴다.

## 현재 변경 경계

새 실험 API는 `minimum-hotfix-ab` 기능으로 제한한다. 기존 일반 실행 정책은 실험 결과를 확인한 뒤 채택 여부를 결정한다.
작업 시작 전부터 수정되어 있던 workflow 및 release-plan 문서는 이 실험의 수정 대상이 아니다.

## 2026-09-11 실행 기록: 기준과 첫 진단

- 4195 전용 서버에서 기존 private GUI의 N32를 4/8/11 workers, 각각 3회 실행했다.
  11-worker 중앙값은 16.657초, 8-worker는 22.857초, 강제 N32 4-worker는 73.788초다.
  모든 표본은 최종 응답과 25개 구성원의 첫 표시까지 완료했다. 원시 자료는 primary checkout의
  `_local/research/minimum-cpu-gpu-stage-ab-20260911/results/01-`부터 `09-`까지다.
- **강제 N32의 4-worker 수치를 채택 정책의 기준으로 사용하면 안 된다.**
  `minimum_hotfix_policy.rs`와 기존 후보 문서가 설명하듯 일반 제품은 32 partitions 미만에서
  기존 TF 정책을 유지한다. 이 표본은 낮은 병렬도에서 조합의 회귀를 재현한 대조이며,
  동일 AB WASM의 B/4-worker(기존 TF)를 별도로 측정해야 한다.
- 4-worker의 원본 최소 개수 증명 구간은 약 8초였고, 첫 원본-ID canonical 잔여 질의의
  다음 질의 발행까지 간격이 57~58초였다. 이 간격은 모든 내부 비용의 정확한 프로파일이 아니지만
  회귀가 집중된 질의를 식별한다. 서로 다른 워커 수에서 같은 계산량이라고 가정하지 않는다.
- 새 Rust CPU/GPU 코드의 `cargo check -p clearra-webgpu --features minimum-hotfix-ab`는 통과했다.
  네이티브 test 실행 파일과 전체 WASM 의존성 빌드 프로그램은 Windows 애플리케이션 제어의
  오류 4551로 실행되지 않았다. 이는 test pass나 알고리즘 실패가 아니다.
- CPU 전파 모듈만 직접 WASM으로 컴파일하는 로컬 fixture는 생성했다. 브라우저 진단은 이
  **실제 Rust CPU 모듈**, 별도의 JS 의미 기준, 제품에 추가한 **동일 WGSL**을 대조한다.
  JavaScript WebGPU 호스트로 WGSL을 실행하는 결과는 Rust wgpu bridge의 검증과 구분한다.
  최초 GPU 실행에서 WGSL 예약어 `active`가 컴파일 오류를 냈고 `live_bits`로 수정했다.
- 외부의 누락된 cut 인증서를 복제했다고 주장하지 않는다. 로컬의 정확한 fractional primal을
  검산하고, 원본-ID 160개 행의 669,920개 삼중 조합에서 유효한 RHS=2 cut 200개를 재생성했다.
  이 후보군의 가장 큰 fractional 비용은 1.074711600이며 엄밀한 tight 행만 사용한 것은 아니다.
  엄격한 tight 후보 109개로 제한했을 때 얻은 cut 65개도 준비 기록으로 보존했다.
  각 cut의 세 원본 큐, 빈 삼중 교집합, 정확한 union과 위반량을 저장했다. dual 하한 22는 아직 검증하지 않았다.

현재 구현은 전파 단계의 실험이며 완전한 GPU 최소 탐색기나 제품 채택을 뜻하지 않는다.

## WSL 빌드 복구와 실제 wgpu 경로

사용자가 Windows WDAC/SAC 차단 시 WSL 빌드 사용을 명시적으로 허용했다.
Ubuntu의 Rust 1.97.0 / wasm-bindgen 0.2.126으로 전체 실험 WASM을 빌드했다.
업데이트한 packed-state 입력 검증을 포함한 CPU native 검증 2개도 WSL에서 통과했다
(`wsl-test-current.log`: 2 passed, 0 failed; 227 unrelated tests filtered out).
Windows 앱 제어 정책이나 코드 서명 설정은 변경하지 않았다.

처음 전체 Rust wgpu 연결을 실행할 때 `wgpu-30.0.0/src/backend/webgpu.rs:85`의
`Unexpected error` 패닉이 `future_pop_error_scope`에서 발생했다. GPU 탐색의
무한 루프가 아니라 비동기 연결 Promise가 완료되지 않는 호스트 오류였다.
기존 `geometry_exact_cover_backend.rs`와 같은 방식으로 셰이더의 compilation-info를
검사하고 pipeline error scope를 native 대상으로 제한했다. 로컬 GUI는 패닉 위치와
연결/dispatch의 15초 진단 상한을 표시한다. 상한 도달은 UNKNOWN이다.

CPU/GPU 입력은 후보별로 32개 상태를 묶은 배열을 직접 전달한다. `from_device_words`
추가로 배열을 각 상태의 후보 목록으로 풀었다가 재포장하는 중복을 없앴다.
길이, 후보 수/상태 수, 상한, 선택/배제의 충돌, 마지막 32비트 묶음의 패딩을 검증한다.
전체 WASM을 사용하는 후속 표본은 이전 직접 Rust CPU / JS WebGPU 진단 표본과
별도 artifact로 집계한다. `artifacts/`에 각 새 manifest와 실행 자산을 보관한다.

초기 전파 대조는 4/8/11 workers × CPU flat/sliced 및 GPU flat/sliced × cut off/on × 3,
총 72표본이다. 모든 결과는 같았다. 준비/전송을 포함하고 독립 검산 시간은 분리한
CPU sliced 중앙값은 3.0~4.8ms, GPU sliced는 9.4~16.8ms였다. 이 1,024개 얕은
상태에서는 모든 구성의 conflict가 0이므로 cut의 정수 증명 효과를 판단할 수 없다.
이 표본은 전파 비용과 호스트 비용의 진단이며 전체 탐색의 가속 증거가 아니다.
직접 WebGPU 진단에서 선택된 장치는 Intel gen-12lp 내장 GPU였다.

GPU sliced가 0인 conflict 비트를 반복해서 global atomic OR하는 비용을 발견했다.
같은 shader의 `reserved0` 스위치로 이 연산의 생략을 대조한다. 기본 실험 API의
동작은 아직 원래 방식이며, 승격 전에 실제 Rust wgpu 경로에서 계측한다.

동일 AB WASM의 B/4-worker 제품 기준도 3회 완료했다. 중앙값 45.803초,
범위 44.871~47.562초이며, N32/4-worker의 73.788초와 구분한다.
제품 표본 12개는 모두 최소 크기 25, 원본-ID 구성원 해시
`ca0c7b428c21ecec9728765d1d89485374af93620c14c1ffa050d02e7d1225ce`,
첫 구성원 25개 표시, known alternatives 1 / 전체 대안 수 미확정 / lazy 열거 상태를 유지했다.

후속 bounded frontier의 분기 소유권과 원본-ID 잔여 행렬 변환은 작은 입력의 가능한
해 2,943개를 확인했다. 각 가능한 해는 정확히 한 frontier 소유자에 남는다.
이는 아직 P7 전체 최소성 / canonical A/B의 완료가 아니다.

## GPU 설정 전달 복구와 큰 배치의 메모리 정렬

기존 device cache의 adapter로 `request_device`를 반복하면 브라우저에서 adapter가
이미 소비됐다는 오류가 발생했다. geometry backend가 소유한 장치/queue를 공유하는
feature 전용 연결을 추가했고, 반복 연결과 해제 후 456개 GPU 검증을 통과했다.
작은 입력의 가능한 완성 해 1,056개와 경계 입력 2,312개를 독립 기준과 비교했다.

설정 on/off를 확대하면서 uniform 갱신에 두 오류가 확인됐다. 버퍼에 `COPY_DST`가
없었고, flag 갱신 offset 16은 mode를 가리켰다. `COPY_DST` 및 offset 20으로 고쳤다.
**수정 전 guard off/on 18개 표본은 실제 스위치 적용의 A/B 증거에서 제외한다.**
동일 결과였다는 사실이 성능 스위치가 작동했다는 증거는 아니었다. 이전 원시 자료는
변경하지 않고 보존한다. 수정 후 WASM은
`14bbd1d370607825a0012d78bc7f22ba688d960f71df39d15b448a05fecfd693`이다.

또한 진단 timeout이 먼저 발생해도 진행 중인 Rust async borrow를 해제하지 않도록
host의 상태와 matrix를 실제 Promise 완료 시점까지 유지한다. timeout은 UNKNOWN이다.

32,768 상태는 같은 1,024 상태를 복제한 것이 아니다. 원본 행렬의 빈도가 높은 후보
15개를 기준으로 만든 서로 겹치지 않는 완전한 decision cube 집합이다.
후보별로 상태 word가 연속인 배열에 맞춰 인접 GPU lane을 상태 group 방향으로 배치했다.
cut on, 4·8·11 workers, 각 arm 3회에서 실행+전송 중앙값은 다음과 같다.

| workers | 기존 행 방향 lane | 상태 group 방향 lane | group 방향 + zero atomic guard |
| ---: | ---: | ---: | ---: |
| 4 | 39.615ms | 19.740ms | 20.350ms |
| 8 | 40.480ms | 20.380ms | 20.945ms |
| 11 | 40.345ms | 19.350ms | 18.890ms |

이것은 GPU 실행+전송 구간의 개선이다. 상태 포장·병합·결과 변환까지 포함한 해당
배치의 algorithm 중앙값은 기존 142.595~153.170ms, group 방향 126.300~141.335ms다.
독립 scalar 검산 시간은 별도 기록한다. 0인 atomic 생략만으로는 일관된 개선이
확인되지 않았으므로 기본 선택으로 승격하지 않는다. 물리적 RTX 장치의 사용을
주장하지 않으며, 실제 wgpu 기록은 browserwebgpu / adapter index 0이다.

## 학습과 분기 소유권의 독립 검증

원본 행렬은 불변으로 유지하고 K마다 selector를 배정한다. cardinality 충돌·강제
이유에 해당 selector의 부정을 포함해, K=24에서 배운 내용이 K=25를 무조건 제한하지
않도록 했다. 이것은 Boolean first-UIP 학습과 정확한 cardinality 설명이며, 일반적인
cutting-plane PB 부등식 학습이나 MaxSAT 전체 구현이라고 부르지 않는다.

RHS=2 cut의 이유는 유효한 원본 union cut에서 도출한다. 전체 cut 재검사와 후보별
변경 큐를 비교할 수 있고, backtrack은 되돌린 후보의 cut을 다시 활성화한다.
학습 DB 정리는 현재 reason으로 사용 중인 절과 unit/binary 절을 보존한다. 삭제된
watch를 제거한 후에만 절 ID를 재사용한다.

30,240개 질의를 모든 가능한 해와 대조해 K 증감, 가정 변경, cut 전파, DB 삭제를
확인했다. 이 검증에서 DB 정리가 74회 발생했다. frontier의 두 전파 정책은 가능한
해 5,886개가 정확히 한 소유 분기에 남는지 확인했고, 별도의 brute-force oracle로
312개 최소성·원본-ID lex-first canonical 경로를 확인했다. UNKNOWN 분기는 부정
증명으로 처리하지 않는다. 자료는 `incremental-properties.json`,
`frontier-properties.json`, `proof-broker-properties.json`에 있다.

## Cut 하한과 Boolean 학습의 실제 상호작용

재생성한 cut의 효과는 로컬 HiGHS LP 제안 뒤 별도의 정수 산술로 확인했다.
이는 Qnia GUI / minimals / CP-SAT를 다시 측정한 것이 아니다. 원본 5,040개 큐를
모두 덮는 fractional primal과, 후보별 초과 load를 차감한 dual 인증을 보존했다.

| 원본-ID relaxation | 인증 dual 값 | 인증 primal 값 | 인증 정수 올림 하한 |
| --- | ---: | ---: | ---: |
| 기본 1,602행 | 20.932363332 | 20.932397480 | 21 |
| 기본 + 유효 cut 200개 | 21.241645570 | 21.241667381 | 22 |

외부 자료의 21.331027833을 재현했다고 주장하지 않는다. 이것은 이번에 검증한
별도 200개 cut 집합의 결과다. 해당 고정 relaxation의 fractional primal이 있으므로
이 relaxation만 더 빨리 최적화해서 root 정수 하한 25에 도달할 수는 없다.
분기 뒤 조건부 하한, 추가 cut, 더 강한 정수 추론은 별도 가능성이다.

K≤24 / 질의당 5초 / 4·8·11 workers / arm별 3회의 cut off/on은 모두 UNKNOWN이었다.
중복 분기 소유는 없었고, 최초 17/33/45개 frontier 중 어떤 질의도 모든 부정 receipt를
회수하지 못했다. cut은 다수의 충돌을 발견했지만 이 표본의 강제 선택 횟수는 0이었다.
따라서 현재 RHS=2의 unit 전파와 Boolean first-UIP만으로 LP 하한의 개선을 충분히
이용했다고 볼 수 없다. capacity 종료는 없었다.

학습 절 정리는 실제로 절을 제거하고 저장량을 제한했으나 5초 내 증명 완료를 만들지
못했다. 완료 시간의 가속으로 표기하지 않는다. cut 변경 큐는 전체 스캔 대비 각
workers의 3회 합계 재검사를 다음처럼 줄였다.

| workers | 전체 스캔 | 변경 큐 | 전체 conflict 수(전체 / 변경) |
| ---: | ---: | ---: | ---: |
| 4 | 87,501,980 | 42,622,711 | 159,941 / 174,799 |
| 8 | 115,619,959 | 42,512,457 | 158,092 / 167,160 |
| 11 | 105,398,726 | 43,107,840 | 155,981 / 165,936 |

재검사 감소와 같은 시간 내 conflict 증가를 완료 시간의 speedup과 혼동하지 않는다.
이 병목을 확인한 후에만 fixed-dual 조건부 하한과 cut을 연결하는 추가 후보를 만든다.

## 전송 경로의 중복 작업 복구

큰 배치에서 main thread가 모든 후보 × 모든 상태의 비트를 다시 순회하며 worker
결과를 병합하고, GPU 결과까지 직렬로 candidate 목록으로 풀고 있었다. GPU 커널이
빨라져도 이 두 변환이 전체 시간을 지배했다.

32개 상태 word 경계에 worker 분할을 맞춘 뒤 연속 word를 직접 복사하고, 결과를
같은 경계로 나눠 각 worker에서 디코딩하는 후보를 구현했다. 36,120개의 부분-word
경계와 ID 순서를 독립적인 통짜 pack/unpack 결과와 비교했다. 32,768개의 고유 상태,
cut on, 각 3회, 동일 host 자산 및 서버 session
`6bf1f2a4-362b-4b96-8960-988dd78ffa98`의 algorithm 중앙값은 다음과 같다.

| workers | GPU group lane + 기존 host | GPU + packed 병합 / 병렬 decode | CPU sliced |
| ---: | ---: | ---: | ---: |
| 4 | 172.280ms | 72.505ms | 57.170ms |
| 8 | 158.705ms | 64.145ms | 41.300ms |
| 11 | 151.745ms | 65.560ms | 41.530ms |

GPU 경로 내부의 비용은 크게 줄었지만 이 PC에서는 같은 상태 배치를 CPU가 더 빨리
처리했다. GPU가 제품 최소 탐색을 가속했다고 주장하지 않는다. 이전 session의
GPU 126~141ms와 새 session의 64~73ms를 직접 나눈 speedup도 사용하지 않는다.
대조된 같은 session의 기존 host와 새 host만 비교한다.

8/11 CPU worker 차이가 작은 반면 4 worker는 느렸다. 이를 11 worker의 보편적
우위나 다른 PC의 성능 추정으로 일반화하지 않는다. 원시 자료에는 CPU, 메모리,
논리 프로세서 수, 실제 요청 worker 수, 선택된 adapter, 입력/자산 해시가 포함된다.

고정 cut dual의 조건부 하한 후보는 LP를 매 노드 다시 풀지 않는다. 원본 제약의
정수 가중치를 BigInt로 검산하고, 선택/배제 때문에 늘어나는 확정 하한만 갱신한다.
모든 중간 값이 안전한 정수 범위에 드는지 확인한 후 Number 정수 덧셈을 사용한다.
충돌 이유는 기여한 가정과 K selector에 결속한다. 해당 경로를 포함한 작은 문제
독립 검증은 60,480개 질의와 DB 정리 148회를 통과했다.

## 원본 ID 잔여 축소, 이유 공유, 분기 순환

K=24의 sequential CNF는 보조 변수 5,880개와 기본 절 13,560개를 사용한다.
동일 Base solver에서 encoding만 바꾼 4·8·11 workers × 각 3회 대조는 모두 5초
기한의 UNKNOWN이었다. native PB 이유보다 많은 conflict를 처리했지만 전체 부정
증명의 완료 시간을 확보하지 못했다. 이 결과로 SAT / PB / MaxSAT 전체를 기각하지 않는다.

새 scout가 246개 원본 변수를 그대로 탐색하고 있던 차이를 복구하기 위해,
원본 coverage 열을 보존하면서 현재 선택 / 배제 가정 아래에서만 지배 후보를
추가 제외하는 경로를 구현했다. 이는 영구 절이 아니라 그 질의의 가정이며,
이후 canonical 질의에서 해당 ID를 요구하면 다시 후보로 사용할 수 있다.
원본 root의 246개 후보가 158개로 줄었다. 작은 입력의 모든 부분 가정과 K를
직접 열거한 153,090개 검사에서 존재성 보존을 확인했다.

실제 frontier에서는 88~91개 후보를 제외했다. 동일 5초 예산에서 처리하는 conflict가
증가했으나 여전히 모든 질의가 UNKNOWN이었다. 첫 worker들이 5초를 전부 소모해
17 / 33 / 45개 소유 분기 가운데 4 / 8 / 11개만 시작한 것도 별도로 확인했다.

cardinality 전파가 강제 배제마다 같은 선택 집합의 이유 배열을 복제하고 있었다.
불변 이유 tail을 공유하고 conflict 분석 때만 순회하는 후보를 같은 원본 행렬에서
비교했다. 아래는 각 workers의 3회 합계이며, 할당량은 peak RAM이 아니라 생성한
이유용 정수 배열 원소 수이다. 다른 JS 객체나 중간 배열은 이 카운터에 포함하지 않는다.

| workers | 이유 복제 원소 수 | 공유 원소 수 | conflict 수: 복제 / 공유 |
| ---: | ---: | ---: | ---: |
| 4 | 752,681,202 | 12,238,300 | 286,459 / 547,168 |
| 8 | 778,416,340 | 17,280,225 | 290,033 / 774,744 |
| 11 | 751,548,174 | 18,957,150 | 284,480 / 848,922 |

각 원소는 4byte다. 전체 정수 증명은 이 표본에서도 5초 내 완료하지 못했다.
공유 이유와 조건부 하한, K 증감, 가정 변경, DB 정리를 포함한 독립 검증은
120,960개 질의 / DB 정리 296회를 통과했다.

200ms 단위로 unfinished 분기를 뒤로 보내되 worker의 학습 상태를 유지하는
후속 대조에서는 모든 17 / 33 / 45개 소유 분기를 시작했다. 동시에 같은 분기를
두 worker가 소유하지 않으며, 기한이 끝난 분기는 UNKNOWN으로 남는다.
기존 긴 slice와 같은 5초 예산에서 어느 쪽도 모든 부정 receipt를 확보하지 못했다.
순환 방식은 공정성과 양성 해를 발견할 기회를 개선하지만, 이 표본에서는 더 많은
context 전환 때문에 conflict 처리량이 소폭 줄었다. 모든 기종의 기본 정책으로 승격하지 않는다.

624개 작은 최소성 / 원본-ID canonical 경로를 독립 brute-force oracle과 대조했다.
17개 분기를 두 번씩 이어 수행하는 ownership 검사와 capacity 분기 보존도 확인했다.
질의별 kernel 결과 캐시는 선택·배제 배열로 결속하며 최대 128개 항목만 유지한다.

## 실제 Rust 엔진에 연결한 frontier 및 작은 GPU 배치

frontier가 이전에 고정점에 도달한 상태도 다시 전파하던 작업을 제거했다. 각 질의에서
전파한 상태 수는 4 workers 109→29, 8 workers 317→53, 11 workers 557→71이었다.
재방문은 각각 84 / 268 / 490에서 4로 줄었다. 비교한 두 경로는 같은 분기 소유권을
유지하고 기존 저장 행렬 Rust 엔진으로 잔여 정수 문제를 해결한다.

하지만 CPU에서 frontier 준비 시간은 약 10 / 21 / 26ms에서 10 / 21 / 25ms로만
줄었다. 이 작은 상태 수에서는 분기 목록과 원본 제약을 다루는 고정 비용이 남는다.
5초 질의의 지배 비용은 수백~수천 ms의 잔여 정수 탐색이며, 이 전파 절감만으로
전체 최소성 증명의 가속을 선언하지 않는다.

큰 상태 배치에 유리했던 GPU 실행 방향을 작은 frontier에 일반화하지 않았다.
CPU, GPU row 방향, GPU state-group 방향을 각각 3회 비교했다. 아래는 2초의 잔여
탐색 예산과 별도로 기록한 frontier 준비 중앙값이다.

| workers | CPU | GPU row 방향 | GPU state-group 방향 |
| ---: | ---: | ---: | ---: |
| 4 | 9.565ms | 55.185ms | 55.960ms |
| 8 | 18.265ms | 89.175ms | 89.725ms |
| 11 | 24.875ms | 112.650ms | 119.675ms |

짧은 GPU dispatch와 readback을 여러 번 반복하는 비용이 작은 frontier에서 크다.
worker의 수나 GPU 내부 상태 수를 늘리기만 해서 전체 증명 비용이 줄어든다고
가정하지 않는다. 장치 / 메모리 / lane 수 / 전송 지연이 다른 PC에서는 별도 계측한다.

K 포화 시에는 개별 배제를 모두 enqueue하지 않고, 아직 덮이지 않은 원본 제약과
cardinality 제약을 합친 충돌 이유를 직접 만들 수 있었다. K selector와 실제 선택·배제
가정만 사용하므로 새 질의에도 학습 범위를 유지한다. 이 후보를 포함한 독립 검증은
241,920개 질의 / DB 정리 816회를 통과했다. 5초의 K≤24 비교에서 cardinality 배제의
중간 이유 배열 생성은 0으로 줄었지만 최종 부정 증명은 계속 UNKNOWN이었다.
일반 PB 부등식 학습을 구현했다고 표기하지 않는다.

## 후속 후보의 적용 범위와 보류 근거

이번의 느린 학습 경로는 즉시 폐기하지 않았다. 원본-ID 잔여 축소, 학습 DB 저장량,
cardinality 이유 복제, K 포화 시 중간 배제 생성, 분기 독점, 질의 재생성 비용을
각각 복구한 뒤 상호작용을 확인했다. 그래도 K≤24의 5초 부정 질의와 입력에서 얻은
28개 상한 다음의 K≤27 / 15초 질의가 끝나지 않았다. 이 결과는 현재 JavaScript
first-UIP scout의 미완료이며, SAT/PB/MaxSAT 알고리즘 계열의 성능 한계가 아니다.

| 후속 방향 | 이번에 확인한 근거 | 적용 판단 |
| --- | --- | --- |
| 고정 200개 cut의 수치 하한을 GPU로 빠르게 계산 | primal 21.241667381과 dual 21.241645570을 원본 행렬에서 인증. 5초 탐색의 추가 bound conflict는 4/8/11 workers의 3회 합계 3/3/0 | 같은 고정 relaxation을 빠르게 푸는 것만으로 root 하한 25를 만들 수 없다. 기본 경로로 승격하지 않음 |
| 더 넓은 cut 탐색 및 잔여 문제별 가중치 재최적화 | 기존 200개 cut은 하한을 높였으나 강제 선택은 0. 고정 dual은 선택 load가 1인 변수에서 거의 강화되지 않음 | 추가 가능성은 유지. 새 cut의 유효성·중복·후속 정수 탐색 감소를 함께 확인해야 하며, 이번에 광범위 GPU cut 탐색기를 구현했다고 하지 않음 |
| 작은 제약 묶음의 BDD/MDD 또는 폭 제한 DD | 고정 LP와는 다른 정수 상관관계를 표현할 수 있는 후보. 전체 exact diagram과 작은 묶음은 다른 범위 | 미계측 보류. 외부의 누락된 treewidth 인증서로 전체 계열을 기각하지 않음 |
| 일반 cutting-plane PB / core-guided MaxSAT | 이번 구현은 K selector가 포함된 Boolean first-UIP와 정확한 cardinality 이유, sequential CNF 대조까지 | 별도 solver 구현·도입과 기존 canonical 질의 계약의 연결이 필요. 현재 scout의 UNKNOWN을 이 계열의 A/B 결과로 대체하지 않음 |
| GPU가 확장·압축·제거까지 맡는 장기 상주 frontier | 실제 WebGPU 전파·전송·CPU 확인과 CPU 소유 frontier 연결까지 구현. 작은 frontier는 왕복 비용이 지배 | 전체 GPU 탐색기는 미구현. 큰 상태 배치의 커널 개선이 제품 종점에서 유리해진다는 증거가 없으므로 기본 전환하지 않음 |

재사용 A/B에서는 새 질의의 solver 준비 비용이 ms에서 약 0.005ms로 줄었지만
입력에서 얻은 K / 가정 질의들은 제한 시간 안에 완료되지 않았다. 후반 11-worker
query-reuse 표본 일부와 결과 집계가 겹쳤으므로 그 표본의 세밀한 처리량 차이는
독립 성능 근거로 사용하지 않는다. 이후 배치에서는 완료 상태를 먼저 확인한 뒤 집계했다.

## 전체 경로에서 발견한 양성 해 대기

첫 전체 행렬 비교는 질의당 15초, 각 workers / arm 3회였다. 새 학습 경로는
모두 첫 K≤27 질의의 UNKNOWN이었다. 기존 저장 행렬 Rust 엔진은 4/11 workers에서
모든 반복의 최소 크기 25와 같은 원본-ID first canonical을 확정했다. 8 workers는
첫 8개의 분기가 예산을 소모해 33개 frontier 전체를 시작하지 못했다.

기존 Rust arm의 4-worker 전체 중앙값은 88.227초, 11-worker는 83.848초였다.
해를 찾은 뒤에도 다른 blocking WASM 호출을 기다리는 비용이 최소성 단계에만
각각 약 30.1초 / 38.5초 있었다. 이는 제품의 4195 N32 / B baseline과 다른 저장
행렬 broker의 결과다. 제품 성능이 이 시간으로 바뀌었다는 뜻이 아니다.

원본 제약과 현재 가정으로 양성 witness를 독립 검산한 후에만 남은 stateless Rust
worker를 종료하고 재생성하는 후보를 추가했다. 해당 receipt는 UNKNOWN으로 남으며,
재생성·초기화가 끝난 뒤 다음 질의를 시작한다. 늦게 도착한 더 큰 witness로 기존의
작은 witness를 덮어쓰지 않는다. 학습 세션은 이 종료 정책으로 임의 초기화하지 않는다.

같은 자산 / session `c7081d8b-3fb2-4731-9ecd-7c272ff53cda`, 5초 예산의
입력 유래 K≤27에서 다음과 같이 대조했다. 준비와 worker 재초기화까지 포함한다.

| workers | 기존 대기 | 양성 검산 뒤 회수 | 결과 / 각 3회 |
| ---: | ---: | ---: | --- |
| 4 | 5.075초 | 1.366초 | 양쪽 모두 found |
| 8 | 5.119초 | 5.131초 | 양쪽 모두 UNKNOWN |
| 11 | 5.194초 | 0.488초 | 양쪽 모두 found |

회수 arm에서 양성 발견 뒤 잔여 시간의 중앙값은 4 workers 39.680ms,
11 workers 160.070ms였으며 대부분 실제 재초기화 시간이었다. 8 workers처럼
양성 해를 찾지 못한 경우 이 정책은 부정 증명이나 추가 성능 이득을 만들지 않는다.

회수 정책을 켠 전체 경로의 후속 3회 중앙값은 4 workers 57.670초,
11 workers 46.402초였으며, 모두 최소 크기 25와 아래 같은 원본-ID 집합을 확정했다.
8 workers는 K≤27의 15초 제한 안에 양성 해를 찾지 못했다. 이전 전체 경로와
후속 경로는 host 자산이 다르므로 이 두 중앙값을 동일 artifact A/B의 가속률로
표기하지 않는다. 대기 정책의 직접 대조 근거는 위 같은 session의 off/on이다.

```text
0, 6, 24, 37, 55, 57, 66, 69, 72, 85, 114, 124, 159,
160, 162, 165, 167, 177, 181, 206, 211, 213, 221, 224, 227
```

첫 구현과 대기 복구 후의 총 12개 완료 표본 모두 이 집합을 유지한다. 각 전체 경로는
최소성 질의 4개와 원본-ID canonical 질의 205개를 사용했다. canonical 구간은 후속
표본에서도 4 workers 약 32.5초, 11 workers 약 29.4초였다. 대기 회수만으로 제품의
기존 최선 정책을 넘었다거나 Qnia GUI의 실제 3~5초 기준을 달성했다고 판단하지 않는다.

GPU 분기 제거 경로에는 매 round의 CPU 재계산·비교를 연결했다. 두 결과가 다르면
UNKNOWN 오류로 끝나고 해당 상태를 부정 증명에 사용하지 않는다. 과거 CPU 확인
이전의 작은 frontier 표본은 전파/전송 진단으로 남긴다. 검산 시간도 새 frontier
준비 시간에 포함한다. 이것은 개발 표본 일치만으로 GPU에 증명 권한을 주는 방식이 아니다.

## 워커 수와 분기 구성을 분리한 복구

같은 4/8/11 워커에서 입력 유래 K≤27의 frontier 목표 폭만 `4 × workers`와
고정 16으로 대조했다. whole-child 소유권을 보존하므로 실제 분기 수는 목표를
약간 넘어 각각 17/33/45개 또는 17개였다. 두 arm 모두 양성 회수를 사용했다.
아래는 session `aa497bed-4935-4242-9d76-e7a5db51cf38`의 arm별 3회 중앙값이다.

| workers | 워커 비례 폭 | 고정 폭 16 | 결과 |
| ---: | ---: | ---: | --- |
| 4 | 1.633초 | 1.672초 | 같은 17분기, 양쪽 모두 found |
| 8 | 5.133초 / UNKNOWN | 2.088초 / found | 같은 8워커에서 분기 구성 변경만으로 양성 질의 복구 |
| 11 | 0.849초 | 3.116초 | 양쪽 모두 found, 고정 폭은 오히려 회귀 |

원래 8워커 실패를 단순한 CPU 부족이나 보편적인 8워커 비효율로 결론내릴 수 없다.
분기 구성을 바꾸면서 어려운 초기 분기에 작업이 몰린 영향이 컸다. 반대로 고정 폭
16을 모든 워커 수에 적용해서도 안 된다. 이 설정 값은 해의 크기나 canonical ID를
사용하지 않으며, frontier 구성과 실제 계산 자원 수의 상호작용을 분리하기 위한 것이다.

실패했던 8워커만 같은 고정 폭으로 전체 경로를 추가 3회 확인했다. 모두 최소 크기
25와 앞서 기록한 같은 원본-ID first canonical을 확정했으며, 중앙값 55.270초,
범위 53.328~55.459초였다. 최소성 구간 중앙값은 29.475초, canonical은 25.582초다.
완료된 저장 행렬 전체 표본은 합계 15개이며 모두 같은 canonical 집합을 유지했다.
이 추가 복구는 다른 CPU의 성능 예측이나 기존 제품 N32 정책의 교체 근거가 아니다.

## 최종 확인과 채택 판단

CPU 확인까지 포함한 마지막 작은 frontier 대조의 준비 중앙값은 아래와 같다.
같은 session에서 workers / arm마다 3회이며, 모든 GPU round를 CPU 결과와 비교했다.

| workers | CPU | GPU row 방향 + CPU 확인 | GPU group 방향 + CPU 확인 |
| ---: | ---: | ---: | ---: |
| 4 | 9.645ms | 64.085ms | 55.450ms |
| 8 | 16.955ms | 87.595ms | 90.805ms |
| 11 | 24.250ms | 109.730ms | 115.655ms |

해당 2초 K≤24 질의는 모두 UNKNOWN이었다. 작은 frontier의 GPU 왕복 비용이 큰
결론은 CPU 확인을 연결한 뒤에도 유지됐다. CPU 확인 이전 표본은 전파 진단이며
CPU proof confirmation을 완료한 경로와 구분한다.

- 실험에 반영: 검증된 packed-state 입력, 동일 스냅샷 전파, 큰 배치의 group 방향
  배치와 word 단위 병합·병렬 decode, cut 변경 큐, 이유 공유, K 포화 직접 충돌,
  원본-ID 질의 범위 축소, selector를 통한 학습 범위 보존, 이미 고정된 frontier의
  재전파 제거, 양성 해 검산 뒤 stateless 작업 회수. 모두 선택 경로와 원시 대조를 보존한다.
- 조건부 유지: 작업 순환, 분기 폭, 학습 절 삭제와 질의 재사용은 각각 오버헤드나
  증명 전략과 상호작용했다. 8워커를 복구한 고정 폭도 11워커에는 느렸으므로
  전 PC 공통 기본값으로 올리지 않는다.
- 제품 기본값 유지: TF/N32의 기존 admission을 바꾸지 않는다. 같은 새 기준 세션의
  제품 첫 canonical paint는 4-worker TF 45.803초, 8-worker N32 22.857초,
  11-worker N32 16.657초였다. 과거 14.05초 / 12.86초 기록은 별도 artifact / session
  기준으로 보존하며, Qnia GUI의 실제 계측 3~5초 기준을 달성했다고 보고하지 않는다.

최종 입력 검산은 원본 5,040행과 축소한 1,602행의 논리적 동치, 원본-ID cut 200개의
세 행 근거, 완료된 15개 집합의 전체 원본 큐 coverage 및 동일 canonical 해시를
확인했다. 완성된 proof arm 112개가 각각 반복 0/1/2를 정확히 포함했다.
이 검산을 별도의 UNSAT solver나 배포 승인으로 취급하지 않는다.

새 Rust 파일만 포맷했고, GPU dispatch 차원 상한을 모든 배치 방향에서 검사하도록
정리했다. WSL의 활성 Rust 1.97.0 / wasm-bindgen 0.2.126과 고정 lock으로
`rebuild-wsl.ps1` 전체 실행에 성공했다(`wsl-repro-final.log`). 버전명이 같은 별도
toolchain을 선택하면서 wasm target이 누락된 첫 재현 시도는
`wsl-repro-toolchain-failure.log`에 남겼고, 준비된 활성 toolchain의 실제 버전을
확인하도록 수정했다. Windows 보안 정책은 바꾸지 않았다.

최종 native feature check는 `wsl-check-final.log`에서 통과했다. CPU native 독립
검증은 2개, 실제 Rust wgpu의 최종 의미·경계 검증은 456개 설정을 통과했다.
추가로 241,920개 학습 질의, 153,090개 잔여 축소, 624개 최소성·canonical 경로와
분기 ownership / UNKNOWN 보존, 36,120개 packed 전송 경우를 독립 기준으로 확인했다.
이는 로컬 실험의 검증이며 전사 CI, 릴리즈 게이트, 배포 실행을 추가하지 않았다.

성능 계측에 사용한 Rust WASM은 `14bbd1d370607825a0012d78bc7f22ba688d960f71df39d15b448a05fecfd693`,
최종 정리 후 의미 검증한 WASM은 `ebaa868fe987abc94c036f324642eb45ba1015d0be2285205cb716f164fb5ec8`이다.
각 시점의 자산·manifest와 원본 Rust 소스를 archive에 보존했다. 최종 정리본의
성능을 다시 측정한 것처럼 과거 수치의 artifact를 바꾸지 않는다.

재현 자료는 primary checkout의 `_local/research/minimum-cpu-gpu-stage-ab-20260911/`에 있다.
`README.md`의 실행 순서, `analysis.json`의 artifact / session별 집계,
`result-audit.json`의 입력·반복·정답 검산, `results/`의 불변 원시 자료를 함께 사용한다.

다른 PC용 실행 묶음은 `_local/benchmark-portable/minimum-cpu-gpu-stage-20260911-final.zip`이다.
원시 자료, CPU/GPU 비공개 GUI, 고정 base 자산, 소스 overlay, integrity inventory를
포함한다. 기본 `run.mjs`는 최종 정리본을, `run-measured.mjs`는 후반 실제 계측에
사용한 14bbd1d… WASM과 해당 host 자산을 사용한다. 같은 종점 / 자산으로 다른 PC를
비교할 때는 `MEASURED.md`를 따른다. 모델·dependency cache·인증 자료는 포함하지 않는다.
