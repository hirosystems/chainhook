import { Static, Type } from '@sinclair/typebox';
import {
  BlockIdentifierSchema,
  Nullable,
  RosettaOperationSchema,
  TransactionIdentifierSchema,
} from '../common';
import { StacksTransactionEventSchema } from './tx_events';
import { StacksTransactionKindSchema } from './tx_kind';
import { StacksIfThisSchema } from './if_this';

export const StacksExecutionCostSchema = Type.Optional(
  Type.Object({
    read_count: Type.Integer(),
    read_length: Type.Integer(),
    runtime: Type.Integer(),
    write_count: Type.Integer(),
    write_length: Type.Integer(),
  })
);
export type StacksExecutionCost = Static<typeof StacksExecutionCostSchema>;

export const StacksTransactionReceiptSchema = Type.Object({
  contract_calls_stack: Type.Array(Type.String()),
  events: Type.Array(StacksTransactionEventSchema),
  mutated_assets_radius: Type.Array(Type.String()),
  mutated_contracts_radius: Type.Array(Type.String()),
});
export type StacksTransactionReceipt = Static<typeof StacksTransactionReceiptSchema>;

export const StacksTransactionPositionSchema = Type.Object({
  index: Type.Integer(),
  micro_block_identifier: Type.Optional(BlockIdentifierSchema),
});
export type StacksTransactionPosition = Static<typeof StacksTransactionPositionSchema>;

export const StacksTransactionMetadataSchema = Type.Object({
  description: Type.String(),
  execution_cost: StacksExecutionCostSchema,
  fee: Type.Integer(),
  kind: StacksTransactionKindSchema,
  nonce: Type.Integer(),
  position: StacksTransactionPositionSchema,
  proof: Nullable(Type.String()),
  raw_tx: Type.String(),
  receipt: StacksTransactionReceiptSchema,
  result: Type.String(),
  sender: Type.String(),
  sponsor: Type.Optional(Type.String()),
  success: Type.Boolean(),
  contract_abi: Type.Optional(Type.Any()),
});
export type StacksTransactionMetadata = Static<typeof StacksTransactionMetadataSchema>;

const StacksTransactionSchema = Type.Object({
  transaction_identifier: TransactionIdentifierSchema,
  operations: Type.Array(RosettaOperationSchema),
  metadata: StacksTransactionMetadataSchema,
});
export type StacksTransaction = Static<typeof StacksTransactionSchema>;

export const StacksEventMetadataSchema = Type.Object({
  bitcoin_anchor_block_identifier: BlockIdentifierSchema,
  confirm_microblock_identifier: Nullable(BlockIdentifierSchema),
  pox_cycle_index: Type.Integer(),
  pox_cycle_length: Type.Integer(),
  pox_cycle_position: Type.Integer(),
  stacks_block_hash: Type.String(),

  tenure_height: Nullable(Type.Integer()),

  // Fields included in Nakamoto block headers
  block_time: Nullable(Type.Integer()),
  signer_bitvec: Nullable(Type.String()),
  signer_signature: Nullable(Type.Array(Type.String())),

  // Available starting in epoch3, only included in blocks where the pox cycle rewards are first calculated
  cycle_number: Nullable(Type.Integer()),
  reward_set: Nullable(
    Type.Object({
      pox_ustx_threshold: Type.String(),
      rewarded_addresses: Type.Array(Type.String()),
      signers: Nullable(
        Type.Array(
          Type.Object({
            signing_key: Type.String(),
            weight: Type.Integer(),
            stacked_amt: Type.String(),
          })
        )
      ),
    })
  ),
});
export type StacksEventMetadata = Static<typeof StacksEventMetadataSchema>;

export const StacksEventSchema = Type.Object({
  block_identifier: BlockIdentifierSchema,
  parent_block_identifier: BlockIdentifierSchema,
  timestamp: Type.Integer(),
  transactions: Type.Array(StacksTransactionSchema),
  metadata: StacksEventMetadataSchema,
});
export type StacksEvent = Static<typeof StacksEventSchema>;

export const StacksPayloadSchema = Type.Object({
  apply: Type.Array(StacksEventSchema),
  rollback: Type.Array(StacksEventSchema),
  chainhook: Type.Object({
    uuid: Type.String(),
    predicate: StacksIfThisSchema,
    is_streaming_blocks: Type.Boolean(),
  }),
});
export type StacksPayload = Static<typeof StacksPayloadSchema>;
