import { Static, Type } from '@fastify/type-provider-typebox';

export const StacksNakamotoBlockHeaderSchema = Type.Object({
  version: Type.Integer(),
  chain_length: Type.Integer(),
  burn_spent: Type.Integer(),
  consensus_hash: Type.String(),
  parent_block_id: Type.String(),
  tx_merkle_root: Type.String(),
  state_index_root: Type.String(),
  timestamp: Type.Integer(),
  miner_signature: Type.String(),
  signer_signature: Type.Array(Type.String()),
  pox_treatment: Type.String(),
});
export type StacksNakamotoBlockHeader = Static<typeof StacksNakamotoBlockHeaderSchema>;

export const StacksNakamotoBlockSchema = Type.Object({
  header: StacksNakamotoBlockHeaderSchema,
  // TODO(rafaelcr): Add transactions
  // transactions: Type.Array(StacksTransactionSchema),
});
export type StacksNakamotoBlock = Static<typeof StacksNakamotoBlockSchema>;

export const StacksSignerMessageBlockProposalSchema = Type.Object({
  type: Type.Literal('BlockProposal'),
  data: Type.Object({
    block: StacksNakamotoBlockSchema,
    burn_height: Type.Integer(),
    reward_cycle: Type.Integer(),
  }),
});
export type StacksSignerMessageBlockProposal = Static<
  typeof StacksSignerMessageBlockProposalSchema
>;

export const StacksSignerMessageBlockResponseAcceptedSchema = Type.Object({
  type: Type.Literal('Accepted'),
  data: Type.Object({
    signer_signature_hash: Type.String(),
    sig: Type.String(),
  }),
});
export type StacksSignerMessageBlockResponseAccepted = Static<
  typeof StacksSignerMessageBlockResponseAcceptedSchema
>;

export const StacksSignerMessageBlockResponseRejectedSchema = Type.Object({
  type: Type.Literal('Rejected'),
  data: Type.Object({
    reason: Type.String(),
    reason_code: Type.Union([
      Type.Literal('VALIDATION_FAILED'),
      Type.Literal('CONNECTIVITY_ISSUES'),
      Type.Literal('REJECTED_IN_PRIOR_ROUND'),
      Type.Literal('NO_SORTITION_VIEW'),
      Type.Literal('SORTITION_VIEW_MISMATCH'),
      Type.Literal('TESTING_DIRECTIVE'),
    ]),
    signer_signature_hash: Type.String(),
    chain_id: Type.Integer(),
    signature: Type.String(),
  }),
});
export type StacksSignerMessageBlockResponseRejected = Static<
  typeof StacksSignerMessageBlockResponseRejectedSchema
>;

export const StacksSignerMessageBlockResponseSchema = Type.Object({
  type: Type.Literal('BlockResponse'),
  data: Type.Union([
    StacksSignerMessageBlockResponseAcceptedSchema,
    StacksSignerMessageBlockResponseRejectedSchema,
  ]),
});
export type StacksSignerMessageBlockResponse = Static<
  typeof StacksSignerMessageBlockResponseSchema
>;

export const StacksSignerMessageBlockPushedSchema = Type.Object({
  type: Type.Literal('BlockPushed'),
  data: Type.Object({
    block: StacksNakamotoBlockSchema,
  }),
});
export type StacksSignerMessageBlockPushed = Static<typeof StacksSignerMessageBlockPushedSchema>;

export const StacksSignerMessageSchema = Type.Union([
  StacksSignerMessageBlockProposalSchema,
  StacksSignerMessageBlockResponseSchema,
  StacksSignerMessageBlockPushedSchema,
]);
export type StacksSignerMessage = Static<typeof StacksSignerMessageSchema>;

export const StacksSignerMessageEventSchema = Type.Object({
  type: Type.Literal('SignerMessage'),
  data: Type.Object({
    contract: Type.String(),
    sig: Type.String(),
    pubkey: Type.String(),
    message: StacksSignerMessageSchema,
  }),
});
export type StacksSignerMessageEvent = Static<typeof StacksSignerMessageEventSchema>;
