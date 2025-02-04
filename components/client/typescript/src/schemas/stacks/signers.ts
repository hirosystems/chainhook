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
  block_hash: Type.String(),
  index_block_hash: Type.String(),
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
    signature: Type.String(),
    metadata: Type.Object({
      server_version: Type.String(),
    }),
  }),
});
export type StacksSignerMessageBlockResponseAccepted = Static<
  typeof StacksSignerMessageBlockResponseAcceptedSchema
>;

export const StacksSignerMessageMetadataSchema = Type.Object({
  server_version: Type.String(),
});
export type StacksSignerMessageMetadata = Static<typeof StacksSignerMessageMetadataSchema>;

export const StacksSignerMessageBlockResponseRejectedSchema = Type.Object({
  type: Type.Literal('Rejected'),
  data: Type.Object({
    reason: Type.String(),
    reason_code: Type.Union([
      Type.Object({
        VALIDATION_FAILED: Type.Union([
          Type.Literal('BAD_BLOCK_HASH'),
          Type.Literal('BAD_TRANSACTION'),
          Type.Literal('INVALID_BLOCK'),
          Type.Literal('CHAINSTATE_ERROR'),
          Type.Literal('UNKNOWN_PARENT'),
          Type.Literal('NON_CANONICAL_TENURE'),
          Type.Literal('NO_SUCH_TENURE'),
        ]),
      }),
      Type.Literal('CONNECTIVITY_ISSUES'),
      Type.Literal('REJECTED_IN_PRIOR_ROUND'),
      Type.Literal('NO_SORTITION_VIEW'),
      Type.Literal('SORTITION_VIEW_MISMATCH'),
      Type.Literal('TESTING_DIRECTIVE'),
    ]),
    signer_signature_hash: Type.String(),
    chain_id: Type.Integer(),
    signature: Type.String(),
    metadata: StacksSignerMessageMetadataSchema,
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

export const StacksSignerMessagePeerInfoSchema = Type.Object({
  burn_block_height: Type.Integer(),
  stacks_tip_consensus_hash: Type.String(),
  stacks_tip: Type.String(),
  stacks_tip_height: Type.Integer(),
  pox_consensus: Type.String(),
  server_version: Type.String(),
  network_id: Type.Integer(),
  index_block_hash: Type.String(),
});
export type StacksSignerMessagePeerInfo = Static<typeof StacksSignerMessagePeerInfoSchema>;

export const StacksSignerMessageMockProposalDataSchema = Type.Object({
  peer_info: StacksSignerMessagePeerInfoSchema,
});
export type StacksSignerMessageMockProposalData = Static<
  typeof StacksSignerMessageMockProposalDataSchema
>;

export const StacksSignerMessageMockSignatureDataSchema = Type.Object({
  mock_proposal: StacksSignerMessageMockProposalDataSchema,
  metadata: StacksSignerMessageMetadataSchema,
  signature: Type.String(),
  pubkey: Type.String(),
});
export type StacksSignerMessageMockSignatureData = Static<
  typeof StacksSignerMessageMockSignatureDataSchema
>;

export const StacksSignerMessageMockSignatureSchema = Type.Object({
  type: Type.Literal('MockSignature'),
  data: StacksSignerMessageMockSignatureDataSchema,
});
export type StacksSignerMessageMockSignature = Static<
  typeof StacksSignerMessageMockSignatureSchema
>;

export const StacksSignerMessageMockProposalSchema = Type.Object({
  type: Type.Literal('MockProposal'),
  data: StacksSignerMessagePeerInfoSchema,
});
export type StacksSignerMessageMockProposal = Static<typeof StacksSignerMessageMockProposalSchema>;

export const StacksSignerMessageMockBlockSchema = Type.Object({
  type: Type.Literal('MockBlock'),
  data: Type.Object({
    mock_proposal: StacksSignerMessageMockProposalDataSchema,
    mock_signatures: Type.Array(StacksSignerMessageMockSignatureDataSchema),
  }),
});
export type StacksSignerMessageMockBlock = Static<typeof StacksSignerMessageMockBlockSchema>;

export const StacksSignerMessageSchema = Type.Union([
  StacksSignerMessageBlockProposalSchema,
  StacksSignerMessageBlockResponseSchema,
  StacksSignerMessageBlockPushedSchema,
  StacksSignerMessageMockSignatureSchema,
  StacksSignerMessageMockProposalSchema,
  StacksSignerMessageMockBlockSchema,
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
