# Canonical 구간 분할 후보 C: 4195 직렬 A/B

## 결론

**C는 정확성 대조를 통과했지만 이 P7 입력에서 느려져 승격하지 않는다.**
동일 WASM과 동일 M2 워커 정책에서 M 중앙값 33.616초, C 중앙값 51.277초였다.
증명 wave가 M의 29개에서 C의 77개로 늘고, remote task도 약 3배로 증가했다.
3초 목표는 달성하지 못했다. 이번 결과는 알고리즘 후보를 제외할 근거이며,
전체 CP-SAT 구현이나 성능 개선 성공으로 표현하지 않는다.

현행 배포 main `1fa1918c33213bd2766f1e673da91c2c5c4299bb`에는 이 후보를
포함하지 않는다. 핫픽스 branch에서 C의 기본값은 false이며,
`minimum-hotfix-ab` feature의 명시적 local flag로만 활성화한다.

## 사용자 조건과 실험 경계

- S23 Ultra의 7은 reserved-mode **계산 워커 수**였다. 정책 해석은 8 logical /
  7 compute로 정정하되 이전 7 logical 실험값을 소급 변경하지 않는다.
  사용자의 요청에 따라 모바일 추가 계측은 중단했다.
- 이번 실제 호스트: Windows, Chromium 152, actual/reported logical 12,
  compute workers 11, all-CPU 옵션 없음, dedicated control-only coordinator.
  이는 모바일 에뮬레이션이나 실제 S23 성능 결과가 아니다.
- 입력: `ctk3_w0kCQBjwwAMPPAD37g`, P7, 4L, empty hold, Jstris 180.
- 제품 명령: `clearra pc minimals --lines 4 --board-mask 0x3c0f03c0f --height 4 --pieces 6 --patterns P7 --hold empty --rule jstris-180 --backend cpu --cpu-warmup --workers 11`.
- 알려진 최소 개수 25는 출력 사후 조건일 뿐 solver 입력/hint가 아니다.
- 제품 worker와 전체 하위 worker 경로를 사용하되, 계측 종점은 first canonical
  **worker terminal**이다. 전체 GUI의 필드 첫 paint를 측정한 것은 아니다.
- WASM 다운로드/사전 준비는 실행 시간에서 분리했다. CPU 경쟁·thermal·tiering을
  각각 통제한 실험은 아니므로 모든 실행 시간 편차를 알고리즘 하나로 설명하지 않는다.
- Rust→WASM 빌드만 로컬에서 수행했다. native Rust 제품/테스트 실행은 하지 않았다.
  실제 알고리즘 실행과 별개인 회귀 계약은 branch 전용 CI에 제출한다.

## 변경한 알고리즘

K의 정확한 최적성 증명 뒤, 원본 candidate ID 기준 첫 canonical 집합을 찾는
fixed-K selector 질의만 변경했다. M은 현재 witness의 다음 ID를 `w`라 할 때
`[start, w)` 전체에 더 작은 해가 존재하는지 묻는다. C는 상한을
`start + ceil((w-start)/2)`로 줄여 더 작은 절반을 먼저 묻는다.

- positive: 기존 exact oracle의 witness를 원본 행에 재검증한 뒤 더 작은 ID를
  사용한다. 휴리스틱 답을 exact 답으로 승격하지 않는다.
- negative: 검사한 구간의 완전한 부정 증명만으로 `start`를 올린다. 전체
  `[start,w)`를 닫기 전에는 `w`를 prefix에 확정하지 않는다.
- prefix·K·원본 ID·cancel/memory/unknown 의미와 후속 lazy tie 정책을 보존한다.
  clone에 query strategy를 함께 보관한다.
- M과 C 모두 M2의 동일 중첩 warm repair와 워커 설정을 사용한다. 버퍼/루프/
  전송 프로토콜 미세 최적화나 워커 수 변경은 이번 비교에 포함하지 않았다.

원래 가설은 positive witness가 한 ID씩만 내려오는 경우를 줄이는 것이었다.
하지만 **부정 질의 한 번으로 넓은 구간을 배제할 수 있는 입력**에서는 이분할이
그 일을 여러 번 하게 만든다. 따라서 구간 폭의 로그 감소만 보고 전체 proof가
빨라진다고 가정할 수 없다. 이 fixture는 매번 29→77 wave 증가로 그 반례를 보였다.

## 4195 전용 batch count

로컬 연구 client에 batch count 1~20, 기본 1을 추가했다. 현재 페이지의 order를
반복하며 전체 표본 수는 최대 20개다. command/order/worker/논리 CPU/batch ID를
실행 시작 시 고정하고 각 `sample()` 완료 및 보고서 저장 이후에만 다음을 시작한다.
각 표본은 새 제품 worker들을 소유하며 warmup은 별도로 기록한다.

실행 중 Run 재클릭은 새 batch를 만들지 않는다. Stop은 현재 소유 worker와
후속 반복을 중단한다. 비성공 terminal이나 fixture deadline은 보고서에 남기고
나머지 batch를 시작하지 않는다. 로컬 fixture의 180초 한도는 제품 정책이 아니다.

실제 UI에서 count=21을 실행 전 거부했고, 8개 성능 표본 종료 후에는 count=2의
새 batch를 시작해 Run 재클릭과 Stop을 확인했다. 그 batch는 첫 표본의 명시적
`manual stop`만 기록하고 두 번째 표본을 시작하지 않았다. 이 취소 검증은
성능 성공 표본이나 알고리즘 실패율에 합산하지 않는다.

4195 / 127.0.0.1 외 client 실행은 거부한다. 서버는 45분 lease, 자동 재시작 없음,
기존 listener를 발견하면 실패한다. 4194와 8790은 변경하지 않았다. 비교 harness,
외부 코드, 바이너리, raw matrix와 raw reports는 `_local`에만 두고 CI/배포에 넣지 않는다.

## 동일 바이너리의 모든 성능 표본

WASM SHA256: `ccb8f6f07b1234dae1f4c9689f91b9d8c79b84032ef0e56b6dc927dd0e5fa849`.
Harness SHA256: `b64e6338e758e1c8983b363cddf12780252fb6655f8cc4472f04c081203469ef`.
Manifest runtime identity는 `unverified-local-build`이며 릴리스 바이너리가 아니다.
M은 flag=1, C는 flag=5이다. 외부 Qnia 연구의 core variant C와 혼동하지 않는다.

batch count=2, 순서 `M M / C C / C C / M M`. 동시에 한 표본만 실행했다.

| 순서 | 변형 | command→first terminal (ms) | finalize (ms) | wave | remote tasks |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | M | 29,091.960 | 28,666.825 | 29 | 929 |
| 2 | M | 46,736.450 | 46,288.215 | 29 | 919 |
| 3 | C | 52,160.595 | 51,642.030 | 77 | 2,759 |
| 4 | C | 50,393.525 | 49,791.735 | 77 | 2,752 |
| 5 | C | 47,005.330 | 46,529.350 | 77 | 2,752 |
| 6 | C | 78,623.530 | 78,028.870 | 77 | 2,719 |
| 7 | M | 33,787.490 | 33,211.760 | 29 | 918 |
| 8 | M | 33,443.945 | 32,991.645 | 29 | 911 |

| 변형 | 최소 (ms) | 중앙값 (ms) | 최대 (ms) |
| --- | ---: | ---: | ---: |
| M | 29,091.960 | 33,615.718 | 46,736.450 |
| C | 47,005.330 | 51,277.060 | 78,623.530 |

여덟 표본 모두 원본 ID/normalized key의 동일한 canonical 25개를 반환했고,
기존 source-bound 원본 matrix의 5,040개 큐를 전부 다시 커버했다.
Canonical members hash는 `7314db27276521fe547b76236fd196726c7a40cbc4ef2af2935ea2363df04d8c`.
known alternatives=1, total alternatives=null, enumeration_complete=false를 유지했다.
후속 tie 페이지는 요청하지 않았다.

M의 source/drain/verifier-finish는 각각 279~383 / 45~71 / 74~89ms였다.
C에서는 334~452 / 49~72 / 73~121ms였다. 대부분의 실행 시간은 finalize의
exact proof/canonical work에 남는다. 프로파일의 병렬·중첩 clock을 합산해서
CPU 시간 또는 독립 단계 시간으로 해석하지 않는다. 현재 wave profile에 K-proof와
prefix-query 목적별 terminal 구분은 없으므로 각 wave를 임의로 특정 증명에 배정하지 않는다.

두 변형 모두 remote workers=11, sampled total active peak=11이었다. M은 매번
26/29, C는 매번 74/77 wave에서 all-ready 이전에 첫 작업을 발행했다.
따라서 이번 C 회귀를 1워커 직렬 실행으로 설명할 근거는 없다. 이 수치 자체는
CPU 사용률 100%나 모든 장치의 overhead 부재를 증명하지 않는다.

## 남기는 결정과 재검증 경계

1. C를 이번 핫픽스 성능 개선으로 채택하지 않는다. 기본 비활성 실험과 반례를
   보존하여 컨텍스트 변경 뒤 같은 실험을 반복하지 않는다.
2. G/M1의 기존 거부 및 M2 시작 barrier 개선 기록은 유지한다. 단순 워커 조정과
   정확한 부정 증명의 강화를 구분한다.
3. 다음 알고리즘 축은 더 강한 정수로 검증 가능한 하한과 scope-safe fixed-K
   질의 간 학습 재사용이다. Qnia의 LP 중심 proof가 임의 optimal witness/K에
   약 5초를 쓴 이전 기록은 first canonical 종점과 구분하며 재계측하지 않았다.
4. 추가한 Rust 계약은 독립 brute-force 소형 원본 행들의 첫 canonical 집합 및
   clone 전략 보존을 확인한다. 새 CI 결과는 이 세션에서 기다리거나 읽지 않는다.
5. 3초 목표, 모든 fixture에서의 성능 개선, GUI 첫 paint, 실제 모바일 동등성을
   달성했다고 하지 않는다. 실험 결과는 production gate를 대체하지 않는다.

로컬 원시 결과: `_local/reports/minimum-browser-ab-20260908-v4/`.
검증기: `_local/research/summarize-minimum-browser-ab-20260908.mjs`.
서버: `_local/research/minimum-ab-server-20260907.mjs`.
소스 client: hotfix worktree `_local/research/minimum-browser-ab/client.ts`.

종료 시 소유한 4195 탭을 닫고 listener의 PID와 서버 명령 소유권을 확인한 뒤
그 서버만 종료했다. 이후 4195 listener 부재, 기존 4194/8790 listener 유지를
읽기 전용으로 확인했다. 재실행 시 같은 로컬 서버 명령이 45분 lease로 다시 열린다.

## 별도 production 재실행

main `1fa1918`은 Vault 참조 보존/복구 readiness/닫힌 진단 코드만 수정했다.
아래 신규 실행은 제출만 확인했고 결과는 기다리거나 조회하지 않았다.

- [보호된 Oracle/Cloud 복구 34209478041](https://github.com/daejunnom/Clearra/actions/runs/34209478041)
- [전체 릴리스 게이트 34209481859](https://github.com/daejunnom/Clearra/actions/runs/34209481859)
- [Pages 게시 대기열 34209519274](https://github.com/daejunnom/Clearra/actions/runs/34209519274)

각 배포의 최초 승인과 보호 조건은 유지한다. 승격 여부는 해당 CI/배포의
독립 검증 결과로만 판단하며 본 A/B는 어떠한 배포 권한도 부여하지 않는다.
