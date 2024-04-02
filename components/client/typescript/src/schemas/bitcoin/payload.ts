import { Static, Type } from '@sinclair/typebox';
import {
  Nullable,
  BlockIdentifierSchema,
  TransactionIdentifierSchema,
  RosettaOperationSchema,
} from '../common';

export const BitcoinInscriptionRevealedSchema = Type.Object({
  content_bytes: Type.String(),
  content_type: Type.String(),
  content_length: Type.Integer(),
  inscription_number: Type.Object({
    jubilee: Type.Integer(),
    classic: Type.Integer(),
  }),
  inscription_fee: Type.Integer(),
  inscription_id: Type.String(),
  inscription_input_index: Type.Integer(),
  inscription_output_value: Type.Integer(),
  inscription_pointer: Nullable(Type.Integer()),
  inscriber_address: Nullable(Type.String()),
  delegate: Nullable(Type.String()),
  metaprotocol: Nullable(Type.String()),
  metadata: Nullable(Type.Any()),
  parent: Nullable(Type.String()),
  ordinal_number: Type.Integer(),
  ordinal_block_height: Type.Integer(),
  ordinal_offset: Type.Integer(),
  satpoint_post_inscription: Type.String(),
  transfers_pre_inscription: Type.Integer(),
  curse_type: Nullable(Type.Any()),
  tx_index: Type.Integer(),
});
export type BitcoinInscriptionRevealed = Static<typeof BitcoinInscriptionRevealedSchema>;

export const BitcoinInscriptionTransferredSchema = Type.Object({
  destination: Type.Object({
    type: Type.Union([
      Type.Literal('transferred'),
      Type.Literal('spent_in_fees'),
      Type.Literal('burnt'),
    ]),
    value: Type.Optional(Type.String()),
  }),
  ordinal_number: Type.Integer(),
  satpoint_pre_transfer: Type.String(),
  satpoint_post_transfer: Type.String(),
  post_transfer_output_value: Nullable(Type.Integer()),
  tx_index: Type.Integer(),
});
export type BitcoinInscriptionTransferred = Static<typeof BitcoinInscriptionTransferredSchema>;

export const BitcoinOrdinalOperationSchema = Type.Object({
  inscription_revealed: Type.Optional(BitcoinInscriptionRevealedSchema),
  inscription_transferred: Type.Optional(BitcoinInscriptionTransferredSchema),
});
export type BitcoinOrdinalOperation = Static<typeof BitcoinOrdinalOperationSchema>;

export const BitcoinOutputSchema = Type.Object({
  script_pubkey: Type.String(),
  value: Type.Integer(),
});
export type BitcoinOutput = Static<typeof BitcoinOutputSchema>;

export const BitcoinBrc20DeployOperationSchema = Type.Object({
  deploy: Type.Object({
    tick: Type.String(),
    max: Type.String(),
    lim: Type.String(),
    dec: Type.String(),
    address: Type.String(),
    inscription_id: Type.String(),
    self_mint: Type.Boolean(),
  }),
});
export type BitcoinBrc20DeployOperation = Static<typeof BitcoinBrc20DeployOperationSchema>;

export const BitcoinBrc20MintOperationSchema = Type.Object({
  mint: Type.Object({
    tick: Type.String(),
    amt: Type.String(),
    address: Type.String(),
    inscription_id: Type.String(),
  }),
});
export type BitcoinBrc20MintOperation = Static<typeof BitcoinBrc20MintOperationSchema>;

export const BitcoinBrc20TransferOperationSchema = Type.Object({
  transfer: Type.Object({
    tick: Type.String(),
    amt: Type.String(),
    address: Type.String(),
    inscription_id: Type.String(),
  }),
});
export type BitcoinBrc20TransferOperation = Static<typeof BitcoinBrc20TransferOperationSchema>;

export const BitcoinBrc20TransferSendOperationSchema = Type.Object({
  transfer_send: Type.Object({
    tick: Type.String(),
    amt: Type.String(),
    sender_address: Type.String(),
    receiver_address: Type.String(),
    inscription_id: Type.String(),
  }),
});
export type BitcoinBrc20TransferSendOperation = Static<
  typeof BitcoinBrc20TransferSendOperationSchema
>;

export const BitcoinBrc20OperationSchema = Type.Union([
  BitcoinBrc20DeployOperationSchema,
  BitcoinBrc20MintOperationSchema,
  BitcoinBrc20TransferOperationSchema,
  BitcoinBrc20TransferSendOperationSchema,
]);

export const BitcoinTransactionMetadataSchema = Type.Object({
  ordinal_operations: Type.Array(BitcoinOrdinalOperationSchema),
  brc20_operation: Type.Optional(BitcoinBrc20OperationSchema),
  outputs: Type.Optional(Type.Array(BitcoinOutputSchema)),
  proof: Nullable(Type.String()),
});
export type BitcoinTransactionMetadata = Static<typeof BitcoinTransactionMetadataSchema>;

export const BitcoinTransactionSchema = Type.Object({
  transaction_identifier: TransactionIdentifierSchema,
  operations: Type.Array(RosettaOperationSchema),
  metadata: BitcoinTransactionMetadataSchema,
});
export type BitcoinTransaction = Static<typeof BitcoinTransactionSchema>;

export const BitcoinEventSchema = Type.Object({
  block_identifier: BlockIdentifierSchema,
  parent_block_identifier: BlockIdentifierSchema,
  timestamp: Type.Integer(),
  transactions: Type.Array(BitcoinTransactionSchema),
  metadata: Type.Any(),
});
export type BitcoinEvent = Static<typeof BitcoinEventSchema>;
