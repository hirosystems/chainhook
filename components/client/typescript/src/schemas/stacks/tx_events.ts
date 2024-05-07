import { Static, Type } from '@sinclair/typebox';

export const StacksTransactionEventPositionSchema = Type.Object({ index: Type.Integer() });
export type StacksTransactionEventPosition = Static<typeof StacksTransactionEventPositionSchema>;

export const StacksTransactionNftMintEventSchema = Type.Object({
  type: Type.Literal('NFTMintEvent'),
  position: StacksTransactionEventPositionSchema,
  data: Type.Object({
    asset_identifier: Type.String(),
    raw_value: Type.String(),
    recipient: Type.String(),
  }),
});
export type StacksTransactionNftMintEvent = Static<typeof StacksTransactionNftMintEventSchema>;

export const StacksTransactionNftTransferEventSchema = Type.Object({
  type: Type.Literal('NFTTransferEvent'),
  position: StacksTransactionEventPositionSchema,
  data: Type.Object({
    asset_identifier: Type.String(),
    raw_value: Type.String(),
    recipient: Type.String(),
    sender: Type.String(),
  }),
});
export type StacksTransactionNftTransferEvent = Static<
  typeof StacksTransactionNftTransferEventSchema
>;

export const StacksTransactionNftBurnEventSchema = Type.Object({
  type: Type.Literal('NFTBurnEvent'),
  position: StacksTransactionEventPositionSchema,
  data: Type.Object({
    asset_identifier: Type.String(),
    raw_value: Type.String(),
    sender: Type.String(),
  }),
});
export type StacksTransactionNftBurnEvent = Static<typeof StacksTransactionNftBurnEventSchema>;

export const StacksTransactionFtTransferEventSchema = Type.Object({
  type: Type.Literal('FTTransferEvent'),
  position: StacksTransactionEventPositionSchema,
  data: Type.Object({
    amount: Type.String(),
    asset_identifier: Type.String(),
    recipient: Type.String(),
    sender: Type.String(),
  }),
});
export type StacksTransactionFtTransferEvent = Static<
  typeof StacksTransactionFtTransferEventSchema
>;

export const StacksTransactionFtMintEventSchema = Type.Object({
  type: Type.Literal('FTMintEvent'),
  position: StacksTransactionEventPositionSchema,
  data: Type.Object({
    amount: Type.String(),
    asset_identifier: Type.String(),
    recipient: Type.String(),
  }),
});
export type StacksTransactionFtMintEvent = Static<typeof StacksTransactionFtMintEventSchema>;

export const StacksTransactionFtBurnEventSchema = Type.Object({
  type: Type.Literal('FTBurnEvent'),
  position: StacksTransactionEventPositionSchema,
  data: Type.Object({
    amount: Type.String(),
    asset_identifier: Type.String(),
    sender: Type.String(),
  }),
});
export type StacksTransactionFtBurnEvent = Static<typeof StacksTransactionFtBurnEventSchema>;

export const StacksTransactionSmartContractEventSchema = Type.Object({
  type: Type.Literal('SmartContractEvent'),
  position: StacksTransactionEventPositionSchema,
  data: Type.Object({
    contract_identifier: Type.String(),
    raw_value: Type.String(),
    topic: Type.String(),
  }),
});
export type StacksTransactionSmartContractEvent = Static<
  typeof StacksTransactionSmartContractEventSchema
>;

export const StacksTransactionStxTransferEventSchema = Type.Object({
  type: Type.Literal('STXTransferEvent'),
  position: StacksTransactionEventPositionSchema,
  data: Type.Object({
    amount: Type.String(),
    sender: Type.String(),
    recipient: Type.String(),
  }),
});
export type StacksTransactionStxTransferEvent = Static<
  typeof StacksTransactionStxTransferEventSchema
>;

export const StacksTransactionStxMintEventSchema = Type.Object({
  type: Type.Literal('STXMintEvent'),
  position: StacksTransactionEventPositionSchema,
  data: Type.Object({
    amount: Type.String(),
    recipient: Type.String(),
  }),
});
export type StacksTransactionStxMintEvent = Static<typeof StacksTransactionStxMintEventSchema>;

export const StacksTransactionStxLockEventSchema = Type.Object({
  type: Type.Literal('STXLockEvent'),
  position: StacksTransactionEventPositionSchema,
  data: Type.Object({
    locked_amount: Type.String(),
    unlock_height: Type.String(),
    locked_address: Type.String(),
  }),
});
export type StacksTransactionStxLockEvent = Static<typeof StacksTransactionStxLockEventSchema>;

export const StacksTransactionStxBurnEventSchema = Type.Object({
  type: Type.Literal('STXBurnEvent'),
  position: StacksTransactionEventPositionSchema,
  data: Type.Object({
    amount: Type.String(),
    sender: Type.String(),
  }),
});
export type StacksTransactionStxBurnEvent = Static<typeof StacksTransactionStxBurnEventSchema>;

export const StacksTransactionDataVarSetEventSchema = Type.Object({
  type: Type.Literal('DataVarSetEvent'),
  data: Type.Object({
    contract_identifier: Type.String(),
    var: Type.String(),
    new_value: Type.Any(),
  }),
});
export type StacksTransactionDataVarSetEvent = Static<
  typeof StacksTransactionDataVarSetEventSchema
>;

export const StacksTransactionDataMapInsertEventSchema = Type.Object({
  type: Type.Literal('DataMapInsertEvent'),
  data: Type.Object({
    contract_identifier: Type.String(),
    map: Type.String(),
    inserted_key: Type.Any(),
    inserted_value: Type.Any(),
  }),
});
export type StacksTransactionDataMapInsertEvent = Static<
  typeof StacksTransactionDataMapInsertEventSchema
>;

export const StacksTransactionDataMapUpdateEventSchema = Type.Object({
  type: Type.Literal('DataMapUpdateEvent'),
  data: Type.Object({
    contract_identifier: Type.String(),
    map: Type.String(),
    key: Type.Any(),
    new_value: Type.Any(),
  }),
});
export type StacksTransactionDataMapUpdateEvent = Static<
  typeof StacksTransactionDataMapUpdateEventSchema
>;

export const StacksTransactionDataMapDeleteEventSchema = Type.Object({
  type: Type.Literal('DataMapDeleteEvent'),
  data: Type.Object({
    contract_identifier: Type.String(),
    map: Type.String(),
    deleted_key: Type.Any(),
  }),
});
export type StacksTransactionDataMapDeleteEvent = Static<
  typeof StacksTransactionDataMapDeleteEventSchema
>;

export const StacksTransactionEventSchema = Type.Union([
  StacksTransactionFtTransferEventSchema,
  StacksTransactionFtMintEventSchema,
  StacksTransactionFtBurnEventSchema,
  StacksTransactionNftTransferEventSchema,
  StacksTransactionNftMintEventSchema,
  StacksTransactionNftBurnEventSchema,
  StacksTransactionStxTransferEventSchema,
  StacksTransactionStxMintEventSchema,
  StacksTransactionStxLockEventSchema,
  StacksTransactionStxBurnEventSchema,
  StacksTransactionDataVarSetEventSchema,
  StacksTransactionDataMapInsertEventSchema,
  StacksTransactionDataMapUpdateEventSchema,
  StacksTransactionDataMapDeleteEventSchema,
  StacksTransactionSmartContractEventSchema,
]);
export type StacksTransactionEvent = Static<typeof StacksTransactionEventSchema>;
