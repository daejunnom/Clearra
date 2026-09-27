# v0.8.1 Web 조건부 relation의 단일 owner와 bounded peer

## 범위와 권위

소스는 `codex/v081-selective-source-ci-20260927`의
`d35db7cb2aafb4df86863d4fa95b87768cd8c7a9` 이후 변경이다. 이 문서와 같은
커밋 및 비게시 CI의 exact `head_sha`가 새 검증 범위를 결정한다.
직전 SHA의 CI [36326518721](https://github.com/daejunnom/Clearra/actions/runs/36326518721)는
Core, native-products, WASM ABI, surfaces 네 작업 모두 성공했다. 이 결과를 새
peer 변경의 CI 성공으로 사용하지 않는다.

이 단계는 기능과 소유권 구현이다. 벤치마크, 성능 채택, P7P4 ABBA, benchmark
binary, v0.9.0/TB 업그레이드, main promotion, 4194 또는 실제 배포를 진행하지 않는다.
다섯 profile의 기존 qualified legal-board/조건부 pack도 재생성·재자격 검증하지 않는다.

## 소유권과 실행

이전 분산 경로에서는 첫 verifier만 전체 조건부 pack을 소유하고 다른 verifier는
정확 경로를 썼다. 모든 worker로 파일을 전송하면 독립 WASM 메모리마다 같은 자료를
파싱·보유하므로 이 방법을 쓰지 않는다.

| 자료 | owner | 사용 |
| --- | --- | --- |
| 전체 signed 조건부 pack와 완결 occupancy index | root WASM 하나 | 직렬 탐색 및 peer의 bounded batch 조회 |
| canonical context seed | root에서 파생, 각 peer에 작은 사본 | 같은 실제 entry/원행 frame/window를 한 번 바인딩 |
| occupancy-conditioned 레코드 캐시 | 각 verifier의 bounded owner | 조건 일치만 재사용, context별 entry pose slice 공유 |
| pending/sent/miss 목록 | 각 verifier | bounded 중복 억제; miss는 UNSAT가 아님 |
| 실제 exit continuation, exact cache와 scratch | 기존 solver workspace | 기존 Boolean/전체 증거 의미 보존 |

native의 `RelationSource::Full`은 기존 pack의 레코드를 빌려 쓰며 hot lookup에
새 Mutex나 Arc 복제를 넣지 않는다. Web의 `RelationSource::Peer`는 동일한 정확
레코드를 bounded cache에서 제공한다. exit의 전역 합성, ordered kick, Boolean
목표 shortcut, 기존 exact fallback은 원래 solver를 그대로 사용한다.

cache miss는 기다리지 않고 정확 탐색을 실행한다. 최대 64개를 모아 verifier의
기존 8ms host quantum에서 전송하고, root는 WASM 실행 사이에 불변 index를
조회한다. 응답은 뒤 질의에서 사용할 캐시에 원자적으로 넣는다. 한 peer는 한
batch만 in-flight로 유지한다. 응답이 없어도 다음 정확 탐색을 막지 않으며 큐가
포화되면 추가 optional 조회만 생략한다. 탐색 중 HTTP를 호출하지 않는다.
progressive worker readiness, 요청 worker 수, durable executable delegation,
canonical reducer와 결과 정렬 의미를 새 provider별로 재정의하지 않는다.

## 신뢰·세션·실패

`CLLP0001`은 다운로드 가능한 자산이 아니라 신뢰된 in-app 전송 형식이다.
전체 파일을 signed catalog로 자격 검증한 owner만 seed/응답을 생성한다.
수신 peer는 현재 built-in rule, profile, generation, statement, 완전 파일 digest,
bounded proof를 source-embedded 검증 권위에 대조한다. 작은 wire의 SHA-256은
전송 손상 검출용이며 서명이나 외부 데이터 자격을 대신하지 않는다.

lookup은 context의 전체 레코드 dependency union으로만 점유를 투영한다.
실제 lock/exit의 합성에는 원래 물리 보드를 사용한다. 원래 행 삭제 frame,
미노, entry 집합 또는 창이 다르면 hit로 재해석하지 않는다.

초기 seed 미설치·손상은 정확 fallback이다. 설치 후 reply의 checksum/구조/세대,
in-flight query 결합 또는 중첩 조건 결과가 어긋나면 캐시를 poison하고 현재
검색 결과를 무효화한다. host가 이 import 오류를 삼켜서는 안 된다. 이미 sealed
partial이 있고 현재 batch가 idle인 경우에도 pool 전체를 fail-close한다.
job/profile 변경·opt-out·취소 뒤 이전 session의 메시지는 버린다.
full pack과 peer를 같은 realm/profile에 동시에 설치하지 않으며 active workspace의
lease가 있으면 교체·삭제를 거절한다.
기존 WASM verifier의 정상 `finish`는 workspace owner를 제거해 이 lease를 해제한다.
이 생명주기는 소스로 확인했으며 실제 브라우저 반복 실행 검증을 대신하지 않는다.

## 크기와 자원 계약

- context 최대 256개, wire 최대 256KiB, batch 최대 64개.
- cache 레코드 최대 256개, pending/sent 각각 64개, provider-miss memo 최대 128개.
- peer당 1~2MiB 예약: context index, control 배열, 레코드 cache뿐 아니라 입력·
  decoded reply·전송 버퍼의 상한도 포함한다. entry slice는 context 내부에서 공유한다.
- root는 signed resident upper bound, 전체 peer 예약(합계 최대 16MiB), legal-board
  synopsis와 root transient reserve를 128MiB envelope에 함께 넣는다.
- 예산이 부족하면 optional peer를 끈다. worker 수를 몰래 낮추거나 전체 pack을
  worker별로 복제하지 않는다.

이 값은 admission/논리 allocation 예산이다. 실제 qualified 자료의 allocator,
WASM page, JS buffer 및 OS aggregate peak 128MiB 검증은 아직 `Open`이다.
시간 계측을 추가하지 않았으며 성능 이득도 주장하지 않는다.

## 검증과 다음 경계

Web 타입 검사와 17개 TypeScript 계약 파일을 통과했다. root/검증 worker entry point를
모두 typecheck 범위에 포함한다. 변경 계약은 모든 peer의
동일 owner 조회, opt-out 뒤 stale 메시지 거절, 예산에 맞지 않는 optional peer를
생략해도 요청 worker 유지, idle 상태에서 broker 오류의 전체 결과 무효화,
직렬 fallback에서 이미 설치된 root pack의 재설치 없음까지 포함한다.
네이티브 `clearra-wasm-abi` typecheck도 통과했다.

| 집중 검증 | 관측 | 한계 |
| --- | --- | --- |
| 네이티브 Core 조건부 경로 | 46개 통과, 새 peer 테스트 6개 포함; 직접 감독 정상 종료 | 기존 qualified profile 자료의 재검증 아님 |
| peer/full owner 차분 | 1~6L·32개 작은 보드의 정확 lock/exit를 비교, 즉시 miss fallback 및 batch 후 동일 결과 | 실제 qualified pack·대형 제품 family smoke가 아님 |
| 도달성 회귀 | 26개 통과, 로컬 전체 profile 생성 자료용 1개 ignored | full-owner composer/공유 템플릿의 집중 증거; 실제 브라우저 실행 아님 |
| Web 타입·계약 | 17개 계약 파일 통과, 1/2/7/11/16 worker 예약·17 worker optional-off 포함 | mock transport이며 WASM/OS peak 측정 아님 |
| 비게시 CI 경계 | 기존 4개 Node 계약 통과 | 새 commit의 CI 성공과 별개 |
| 네이티브 ABI | typecheck 통과, 감독 정상 종료 | ABI unit-test 실행 파일 검증은 아래 사유로 미완 |

Core 집중 실행 감독 영수증은 `1790522323509518100-38220-runtime.json`,
reachability 회귀는 `1790522348281010800-39028-runtime.json`, native ABI 검사도
`1790521923623433300-17740-runtime.json`에 정상 tree 종료로 기록했다.
새 ABI unit-test 실행 파일 생성은 다른 Cargo 프로세스의 build directory 잠금을
기다리다 `E_CLEARRA_MEMORY_PRESSURE_FAIL_CLOSE`로 종료됐다. 감독된 대기는 peak
40,042,496 bytes를 기록했고 physical critical reserve가 부족했다. 새 캐시의 OOM이나
알고리즘 성능 실패로 해석하지 않는다. 해당 영수증
`1790522354008209700-34276-runtime.json`은 owned tree 정지, 종료 descendant 0,
automatic retry false다. 외부 Cargo/rustc를 종료하거나 자원 한도를 바꾸지 않았다.

WASM target 검사는 Windows가 `serde_derive` DLL을 `os error 4551`로 차단했다.
수동 재시도 1회에서도 같았으며 WSL/정책 우회/자원 변경 재시도를 하지 않았다.
로컬 WASM 산출물이나 브라우저 실행 완료로 기록하지 않는다. 비게시 CI에서
해당 타깃과 새 ABI 집중 테스트를 별도로 검사한다.

남은 경계:

1. exact source의 비게시 WASM/ABI CI와 실제 qualified 자료를 이용한 여러 verifier
   브라우저 smoke, 실제 hit/fallback 및 취소/교체 readback.
2. PC·Build candidate universe/reducer, minimum·lazy ties, replay/copy/page와
   CLI/GUI/Desktop/Discord의 실제 surface parity.
3. native Cargo root의 정상 종료 뒤 finite descendant drain 미완료를 별도 보존.
4. 성능 ABBA, active-session peak, release acceptance, 배포와 rollback/readback은
   다음 단계이며 이 기능 구현/집중 검증으로 자동 완료 처리하지 않는다.
