# PC4 upstream graph-block sidecar 요청안

## 목적

Clearra가 기존 `graph.bin`과 `graph_offsets*.u32.bin`을 변경하지 않고, 큰 온라인
전체-PC/Setup 탐색의 HTTP Range 왕복과 tail을 줄이기 위한 **선택적 보조 인덱스**를
요청한다. V*, policy, Krylov 또는 최선 수 데이터는 사용하지 않는다. `.safetensors`
전환이나 511MB graph 재배치도 요구하지 않는다.

## 프로필별 독립 파일

다섯 킥 프로필은 독립적으로 제공할 수 있어야 한다. 한 프로필만 준비되어도 그
프로필만 활성화하고 나머지는 기존 exact Range 또는 `not_qualified`로 남긴다. 파일명은
upstream 관례에 맞출 수 있지만 manifest에서 아래 의미를 명시해야 한다.

- Jstris 180의 첫 WAN 후보: K=16
- 각 엔트리는 기존 graph record ordinal `0, K, 2K, ... field_count`의
  `graph.bin` byte offset인 little-endian u32
- 마지막 엔트리는 항상 `graph.bin` byte length
- 16-byte header: ASCII `GBLKIDX1`, little-endian version `1`, little-endian
  `field_count`
- 본문 길이: `4 * (ceil(field_count / K) + 1)` bytes
- 각 인접 offset은 비감소이고, 한 block span은 65,536 bytes 이하

## manifest 결박

sidecar 자체만으로 다른 세대/프로필에 재사용할 수 없도록 다음 값을 함께 제공해야 한다.

- profile/rule/kick-table identity
- resolved dataset generation identity(고정 값을 Clearra 소스에 박지 않고 실행 시 해석)
- K와 field_count
- graph target encoding/word width
- source GOFF content identity와 byte length
- source graph content identity와 byte length
- sidecar content identity와 byte length
- 최대 block span
- 해당 프로필의 completion/provenance identity

Clearra는 런타임에 해석한 immutable revision과 이 결박이 모두 일치할 때만 sidecar를
사용한다. 일부 파일만 있거나 결박이 불완전하면 해당 프로필의 sidecar만 비활성화하며,
다른 프로필까지 막지 않는다.

## 이미 확인한 근거

Jstris real-demand trace의 9,999 graph record를 K=16 block에서 다시 구분해 기존 GOFF
exact range와 byte 및 aggregate SHA-256을 대조했고 모두 일치했다. 8MiB/2,048-entry
LRU 모델과 실제 adapter 모두 11,319회/12,606,042B였으며, 기존 온라인 전송 기록
14,998회/21,016,558B보다 작았다. 이는 local format parity일 뿐 WAN 속도 증거는 아니다.

로컬 파일 A/B에서는 Range RTT가 없어 sidecar가 100,000 lookup에서 오히려 약간
느렸으므로 로컬 제품에는 활성화하지 않았다. 요청의 목적은 HTTP/2 공유 연결 위 여러
단일 Range stream의 왕복/tail 감소를 실제 측정하는 데 있다. 제공 후에도 Clearra는
기존 exact 경로와 A/B하여 이득이 없으면 online sidecar를 활성화하지 않는다.
