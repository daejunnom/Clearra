# v0.8.1 실제 246개 구성원 집합의 전체 복사 기능 경계

## 범위

기준은 `codex/v081-selective-source-ci-20260927`의
`bff8ebacf01cfc04d64bf2a28427255613b6c721`이다. 앞선 실제 Rust→UI 검증의
PC 최소 집합은 한 구성원, Build 필수 집합은 두 구성원이었다. 100개를 넘는
member paging은 모의 자료로만 검사됐으므로 실제 source family를 사용하는
다중 페이지 기능 producer/consumer를 추가했다. 기존 테스트와 증거는 보존한다.

production solver, member page size, candidate universe와 공개 minimum 의미는
바꾸지 않는다. 자산 생성·재자격, TB 업그레이드, 성능 계측/ABBA, benchmark
빌드, main 병합, 포트 교체 또는 배포는 하지 않는다.

## 실제 producer

공통 공개 command ingress와 `WasmCommandRuntime`/App/Core를 사용한다.
initial mask `0x3c0f03c0f`, 높이·목표 4L, 여섯 pieces, P7, SRS+, CPU 1 worker,
TB off의 실제 전체 source family에서 246개 canonical key를 구한다.
실제 key의 초기 Gray 셀과 실제 placement 색으로 246페이지 CTK3 drawing을
만든 뒤 같은 query의 공개 `pc pinned-minimals`에 필수 drawing으로 전달한다.
이 source에는 같은 종류 미노의 경계 모호성이 없는 여섯 서로 다른 pieces가 있다.
source-set hash도 전달하므로 다른 재생성 family를 조용히 받아들이지 않는다.

전체 246개가 필수인 집합은 minimum cardinality도 246이다. 이를 fake candidate
주입이나 축소된 테스트 page size로 만들지 않는다. 실제 기본 100-member
`CoveragePortfolioPageStore`에서 100/100/46의 세 페이지와 전체 selected-set
native CTK3 artifact를 직렬화한다. legal/relation 네 on/off 조합을 검사한다.
이 target은 서명 pack을 설치하지 않으므로 unavailable/fail-open 기능 증거이며
실제 asset hit, worker 병렬성, 자료 자격 또는 성능 증거가 아니다.

## 실제 UI 소비자와 source binding

기존 production payload validator, portfolio export key source 및 clipboard
CTK encoder를 사용한다. page 2에는 100개를 렌더링하는 상황에서도 page 1의
선택 집합 identity를 보존하여 나머지 두 실제 Rust page를 읽고 246개 전체를
복사한다. native artifact와 UI export의 실제 field/order 의미를 대조한다.
추가 페이지 중복·누락, cross-page slice, 완료 후 재사용, stale selection과
중간 취소를 검사한다. 취소는 100개 부분 복사 성공으로 바뀔 수 없다.

fixture schema는 v2다. identity는 실제 compiled product의 final response에서
추출하며 writer가 환경값으로 꾸며내지 않는다. ignored CI producer는 검증되지
않은 local binary identity를 게시하지 않는다. CI compile 시 정확한 source SHA와
engine identity를 고정하고 소비자는 자신의 exact SHA와 모든 response identity를
대조한다. artifact 이름에는 source/run/attempt를 계속 포함한다. 입력 환경 일부만
주어지거나 stale source를 받으면 skip 대신 실패한다. Rust test executable과
공통 UI 함수 기능 증거이며 실제 WASM/browser/Tauri IPC 화면의 증거가 아니다.

## 현재 검증 권위

- Rust focused typecheck의 컴파일은 성공했다. 감독 영수증
  `1790541037004699800-9276-runtime.json`은 종료 시 descendant drain 실패를
  기록했으므로 전체 감독 실행 성공과 구분한다. 최종 owned tree는 정리됐다.
- ordinary focused test executable 생성은 성공했다. 기본 native test stack 실행은
  overflow로 실패했고, 기존 CI의 `RUST_MIN_STACK=16777216` 설정을 사용한
  동일 실행 파일의 기능 검증에서 1 passed, 0 failed, ignored writer 1을 확인했다.
  실제 13개 source/product 요청 및 246-member 네 정책의 세 페이지·canonical
  순서·전체 native CTK artifact가 모두 통과했다. 정상 실행 영수증은
  `1790541662717164300-26804-runtime.json`이다. 자원 한도 증설·OOM 재시도는 아니다.
- CI source 계약 13개와 read-only dependency parser 3개는 passed 16, failed 0,
  skipped 0이다. Node syntax, changed Rust target fmt와 diff check는 통과했다.
- 실제 새 246-member Rust→UI 소비자 실행은 후속 exact-source CI에서 확인할
  Open 항목이다. 실제 browser copy, 전체 surface readback과 릴리스도 Open이다.

현재 CI `36347628083`의 다섯 성공 job은 이 추가 기능 검증을 대신하지 않는다.
그 CI의 read-only library-staging 실패 수정은
[production pool 기록](v081-production-web-pool-functional-2026-09-28.md)에 별도로 남긴다.

## 후속 실제 producer와 UI 소비자 실행

[run 36349394915](https://github.com/daejunnom/Clearra/actions/runs/36349394915)은
exact `26630d246e54642875c3486a5d95bccb1cd53e3d`의 실제 Rust producer와
source/run/attempt-bound artifact 게재를 완료했다. 새 UI 단계는 compiled
identity 객체를 입력 조합 이름의 동명 변수로 가린 테스트 오류 때문에 실패했다.
실제 generation mismatch나 복사 자료 손상이 아니라 객체와 문자열의 잘못된
비교이며, 출처 검사 자체를 없애지 않고 변수명을 분리했다.

동일 CI artifact를 이미 선언된 `v081-product-page-smoke` root로 받아 수정된
production UI 소비자에서 실행했다. 이전 v1 파일은 덮어쓰지 않고 같은 root의
별도 증거 파일로 보존했다. expected source는 producer의 exact `26630d24…`로
유지했고 fixture의 identity를 고치지 않았다. 실제 8개 작은 요청과 네 246-member
정책 모두 UI validator/whole-set export를 통과했다. 네 정책 각각 렌더 대상
100개, 전체 복사 246개, 실제 member pages 3개를 확인했으며 테스트는
1 passed, 0 failed, 0 skipped다. 새 Rust 또는 benchmark 빌드는 하지 않았다.

이는 실제 CI-produced 자료와 공통 UI 함수의 로컬 기능 증거다. 수정된 소비자의
새 exact-source CI 통과, 실제 browser clipboard/Tauri IPC 및 앱 릴리스는
별도 Open이다. 실패한 기존 CI를 성공으로 바꾸거나 출처를 재명명하지 않는다.
