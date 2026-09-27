# v0.8.1 native 도달성 템플릿의 요청별 공유 owner

> 이 문서는 당시 native 단계의 증거를 그대로 보존한다. 이후 Web의 단일
> full owner/bounded peer 구현과 남은 surface 검증은
> [후속 Web owner 기록](v081-web-relation-peer-owner-2026-09-28.md)을 따른다.

## 범위와 권위

이번 단계는 v0.8.1 기능·소유권 구현을 진행하며 벤치마크는 다음 단계로 미룬다.
PC4 Tablebase/v0.9.0 계산·I/O·병렬화는 계속 동결한다. 기존 다섯 profile의
qualified legal-board와 조건부 pack은 수정·재생성·재자격 검증하지 않는다.

소스는 `codex/v081-selective-source-ci-20260927`의 `9a76bf8a` 이후 변경이다.
이 문서와 함께 커밋된 exact source 및 비게시 CI의 `head_sha`로 검증 범위를
확정한다. main, Pages, 4194, CLI/Discord 배포는 이 단계에서 갱신하지 않는다.

이전 대장의 `Reachability template·sky entry` Open 항목 중 **native 공유 owner**를
구현한다. Web의 독립 WASM 메모리를 같은 owner로 바꿨다는 뜻이 아니며,
전체 v0.8.1 완료 또는 메모리·성능 gate 통과를 선언하지 않는다.

## 소유권 경계

`SharedReachabilityTemplates`는 `(width, height, kick profile)`에 결박된 요청 owner다.
각 미노의 컴파일 결과를 `OnceLock<Arc<ReachabilityTemplate>>`에 한 번만 생성한다.
회전 후보의 순서·first-success 의미·180 정책은 기존 컴파일러 그대로다.
하나의 프로필이라는 이유만으로 다른 크기·요청·규칙의 자료를 재사용하지 않는다.

| 자료 | 소유권 | 실행 중 변경 |
| --- | --- | --- |
| shape mask, 이동·회전 후보와 역방향 전이 | 요청별 불변 owner | 미노별 최초 생성만 수행 |
| canonical sky-entry pose | 같은 템플릿의 OnceLock | 필요한 경우 최초 생성만 수행 |
| board-dependent reachability cache | worker workspace | 기존 정책대로 갱신 |
| visited/frontier/BFS·reverse scratch | worker workspace | 다른 worker와 공유하지 않음 |
| conditioned prepared context와 결과 계측 | worker workspace | 기존 generation/profile 경계 유지 |
| StandardBag language/root ID·union memo | worker workspace | worker/epoch-local ID를 전역화하지 않음 |

native branch worker와 마지막 representative 재검증은 같은 `SharedWorkerRequest`를
받는다. representative용 공급 테이블도 동일한 기존 immutable request owner를 쓴다.
owner는 결과 병합이 끝날 때까지 유지되고 다음 실행에는 새 owner를 생성한다.
workspace의 크기·프로필이 바뀌면 stale 공유 템플릿을 사용하지 않고 정확한 private
템플릿을 컴파일한다. 이미 쿼리를 시작한 workspace에 owner를 뒤늦게 붙이지 않는다.

## 메모리 영수증

새 private scope는 `native-worker-exit-private-retained-payload-sum.v2`다.

```text
private worker components 합계
+ shared_standard_bag_request_retained_bytes (요청당 한 번)
+ shared_reachability_template_retained_bytes (요청당 한 번)
```

shared template의 실제 Arc와 같은 allocation을 가리키는 payload만 private 합계에서
제외한다. fallback의 private 템플릿은 제외하지 않는다. 공유 owner의 header, Arc
관리 header, 컴파일된 동적 배열과 lazy sky-entry 배열은 owner에 한 번 기록한다.
worker별 snapshot은 max-fold하고 최종 join·representative 검증 이후 owner의 크기를
다시 확정한다. 한 worker가 sky-entry를 초기화하는 동안 서로 다른 두 snapshot을
빼서 private 크기를 구하지 않는다. BuildUp의 나머지 private 항목도 직접 합산한다.

메모 하네스 schema는 v5로 갱신한다. 가속기 하네스도 같은 v2 memory scope를
사용한다. 누락된 공유 템플릿 항목, 이전 scope, 중복 합산과 exact integer overflow를
거절한다. 기존 영수증의 의미를 소급 변경하거나 새 표본과 합치지 않는다.

이 값은 종료 시 논리 retained accounting이다. OS aggregate commit peak,
serial 전체 메모리, allocator overhead 또는 active accelerator 128MiB 증명이 아니다.
큰 worker-private union memo의 전체 비용을 해결했다고 해석하지 않는다.

## 집중 검증

벤치마크 binary 또는 P7P4 ABBA를 새로 만들거나 실행하지 않았다. 작은 테스트
실행 파일만 기존 단일 Cargo output root에서 컴파일했다.

| 검사 | 관측 | 한계 |
| --- | --- | --- |
| 기본 Core typecheck | 컴파일 통과·감독 정상 종료 | 제품 실행 시간·메모리 계측 아님 |
| native CLI product typecheck | 컴파일 통과·감독 정상 종료 | CLI 실행·배포 readback 아님 |
| native parallel `v081_` | 33개 통과·실행 파일 직접 감독 정상 종료 | bounded 집중 테스트; 전체 family 자격 아님 |
| 기존 reachability 회귀 경로 | 16개 통과·감독 정상 종료, 로컬 생성 pack 전용 1개 ignored | 변경하지 않은 qualified profile 자료를 다시 생성하지 않음 |
| 11개 동시 초기화 | template Arc·sky-entry allocation 하나 및 다음 요청의 별도 owner 확인 | 실제 11-worker P7P4 성능·peak 아님 |
| profile·크기 차분 | 다섯 profile, 높이 1~6, 7미노, 세 보드의 shared/private exact lock 결과 일치 | 전체 가능한 보드의 exhaustive proof 아님 |
| native score 제품 경로 | 직렬/고정·자동 병렬의 canonical identity, coverage, scoring payload 대조 및 새 shared 항목 확인 | CLI/GUI/Discord presenter readback 아님 |
| 하네스·메모리·selector Node 계약 | 25개 통과 | 실제 benchmark를 실행하지 않음 |
| focused CI 계약 | 4개 통과 | CI의 현재 commit 성공과 다름 |
| WASM Core typecheck | 컴파일 완료, 기존 Setup 관련 dead-code 경고 2개 | WASM 빌드·브라우저 smoke가 아니며 감독 경고 별도 |

최종 native 집중 실행의 감독 영수증은
`1790519402185740800-39996-runtime.json`이다. 이는 정상 tree 종료를 확인한
직접 테스트 실행이다. Cargo를 root로 감싼 컴파일/테스트 실행은 일부에서
`tree-not-stopped`, 종료 시 descendant 1개를 기록했고 cleanup 뒤 tree는 정지했다.
관련 영수증 `1790518959854143800-16888-runtime.json`,
`1790519090052774100-39824-runtime.json`,
`1790519202471795800-40892-runtime.json`,
`1790519366903495500-18496-runtime.json`을 정상 감독 실행으로 재분류하지 않는다.
자원 한도를 바꾼 재시도나 감독 정책 완화는 수행하지 않았다.

직전 SHA `9a76bf8a163175762c70a97d8cfa9e3cb6eb0642`의 비게시 CI
[`36324585433`](https://github.com/daejunnom/Clearra/actions/runs/36324585433)는
Core, native-products, WASM ABI, surfaces 네 job 모두 성공했다. 이 결과는
새 공유 템플릿 변경의 CI 권위가 아니다. 새 commit의 비게시 source CI를 따로 확인한다.

## 남은 기능 경계

1. Web 분산 conditioned pack은 여전히 한 verifier가 전체 owner를 가진다.
   모든 verifier에 효과를 제공하려면 독립 WASM 메모리 사이의 공유 조회 경계가
   필요하다. 전체 pack의 worker별 복제나 hot-loop HTTP로 대체하지 않는다.
2. PC·Build의 공통 reducer/candidate universe, minimum/lazy ties,
   replay/copy·page와 CLI/GUI/Desktop/Discord의 실제 parity를 완료한다.
3. native Cargo root의 정상 종료 이후 finite descendant drain은 별도 미완료다.
   집중 테스트 성공으로 감독 실패를 닫지 않는다.
4. 성능 ABBA, 공유 asset peak, exact-SHA acceptance, 배포와 rollback/readback은
   다음 단계다. 이 문서의 좁은 `Locally validated`를 `Qualified`/`Released`로 확대하지 않는다.
