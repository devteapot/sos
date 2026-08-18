#ifndef SOS_CORE_DEV_CREDENTIAL_PROTOCOL_V1_H_
#define SOS_CORE_DEV_CREDENTIAL_PROTOCOL_V1_H_

// Canonical SOS Core development credential protocol v1 wire contract.
// All multi-byte integers are unsigned and encoded in network byte order.
// Request: magic[4], version[1], operation[1], payload_length[2], payload.
// Ack:     magic[4], version[1], status[1].
// A request is complete only after exactly payload_length bytes and EOF.
#define SOS_CORE_DEV_V1_MAGIC_0 0x53
#define SOS_CORE_DEV_V1_MAGIC_1 0x4f
#define SOS_CORE_DEV_V1_MAGIC_2 0x53
#define SOS_CORE_DEV_V1_MAGIC_3 0x4b
#define SOS_CORE_DEV_V1_VERSION 0x01
#define SOS_CORE_DEV_V1_OP_PROBE 0x00
#define SOS_CORE_DEV_V1_OP_SET 0x01
#define SOS_CORE_DEV_V1_OP_CLEAR 0x02
#define SOS_CORE_DEV_V1_OP_STATUS 0x03
#define SOS_CORE_DEV_V1_OP_AGENT_SMOKE 0x04
#define SOS_CORE_DEV_V1_STATUS_OK 0x01
#define SOS_CORE_DEV_V1_STATUS_REJECTED 0x02
#define SOS_CORE_DEV_V1_STATUS_WRONG_PEER 0x03
#define SOS_CORE_DEV_V1_STATUS_PROTOCOL_MISMATCH 0x04
#define SOS_CORE_DEV_V1_STATUS_CONFIGURED 0x05
#define SOS_CORE_DEV_V1_STATUS_EMPTY 0x06
#define SOS_CORE_DEV_V1_REQUEST_HEADER_BYTES 8
#define SOS_CORE_DEV_V1_ACK_BYTES 6
#define SOS_CORE_DEV_V1_MAX_PAYLOAD_BYTES 512

// Golden vectors (hex):
// probe: 53 4f 53 4b 01 00 00 00
// clear: 53 4f 53 4b 01 02 00 00
// status: 53 4f 53 4b 01 03 00 00
// agent-smoke: 53 4f 53 4b 01 04 00 00
// set("sk-or-v1-0123456789abcdef01234567"):
//   53 4f 53 4b 01 01 00 21
//   73 6b 2d 6f 72 2d 76 31 2d 30 31 32 33 34 35 36
//   37 38 39 61 62 63 64 65 66 30 31 32 33 34 35 36 37
// ok:                53 4f 53 4b 01 01
// rejected:          53 4f 53 4b 01 02
// wrong-peer:        53 4f 53 4b 01 03
// protocol-mismatch: 53 4f 53 4b 01 04
// configured:        53 4f 53 4b 01 05
// empty:             53 4f 53 4b 01 06
//
// STATUS and AGENT_SMOKE are compatible v1 extensions. STATUS has no payload and reveals only
// configured versus empty. An older endpoint rejects its unknown opcode with
// protocol-mismatch; a client must reject CONFIGURED/EMPTY for any other op.
// AGENT_SMOKE has no payload and queues one source-embedded, non-secret prompt
// through the production UI submit path only when the credential is configured.

#endif // SOS_CORE_DEV_CREDENTIAL_PROTOCOL_V1_H_
