import { Static, Type } from '@sinclair/typebox';
import { BitcoinIfThisThenThatSchema } from './bitcoin/if_this';
import { StacksIfThisThenThatSchema } from './stacks/if_this';

export const ThenThatFileAppendSchema = Type.Object({
  file_append: Type.Object({
    path: Type.String(),
  }),
});
export type ThenThatFileAppend = Static<typeof ThenThatFileAppendSchema>;

export const ThenThatHttpPostSchema = Type.Object({
  http_post: Type.Object({
    url: Type.String({ format: 'uri' }),
    authorization_header: Type.String(),
  }),
});
export type ThenThatHttpPost = Static<typeof ThenThatHttpPostSchema>;

export const ThenThatSchema = Type.Union([ThenThatFileAppendSchema, ThenThatHttpPostSchema]);
export type ThenThat = Static<typeof ThenThatSchema>;

export const PredicateHeaderSchema = Type.Object({
  uuid: Type.String({ format: 'uuid' }),
  name: Type.String(),
  version: Type.Integer(),
  chain: Type.String(),
});
export type PredicateHeader = Static<typeof PredicateHeaderSchema>;

export const PredicateSchema = Type.Composite([
  PredicateHeaderSchema,
  Type.Object({
    networks: Type.Union([
      Type.Object({
        mainnet: Type.Union([BitcoinIfThisThenThatSchema, StacksIfThisThenThatSchema]),
      }),
      Type.Object({
        testnet: Type.Union([BitcoinIfThisThenThatSchema, StacksIfThisThenThatSchema]),
      }),
    ]),
  }),
]);
export type Predicate = Static<typeof PredicateSchema>;

export const PredicateExpiredDataSchema = Type.Object({
  expired_at_block_height: Type.Integer(),
  last_evaluated_block_height: Type.Integer(),
  last_occurrence: Type.Optional(Type.Integer()),
  number_of_blocks_evaluated: Type.Integer(),
  number_of_times_triggered: Type.Integer(),
});
export type PredicateExpiredData = Static<typeof PredicateExpiredDataSchema>;

export const PredicateStatusSchema = Type.Union([
  Type.Object({
    info: Type.Object({
      number_of_blocks_to_scan: Type.Integer(),
      number_of_blocks_evaluated: Type.Integer(),
      number_of_times_triggered: Type.Integer(),
      last_occurrence: Type.Optional(Type.Integer()),
      last_evaluated_block_height: Type.Integer(),
    }),
    type: Type.Literal('scanning'),
  }),
  Type.Object({
    info: Type.Object({
      last_occurrence: Type.Optional(Type.Integer()),
      last_evaluation: Type.Integer(),
      number_of_times_triggered: Type.Integer(),
      number_of_blocks_evaluated: Type.Integer(),
      last_evaluated_block_height: Type.Integer(),
    }),
    type: Type.Literal('streaming'),
  }),
  Type.Object({
    info: PredicateExpiredDataSchema,
    type: Type.Literal('unconfirmed_expiration'),
  }),
  Type.Object({
    info: PredicateExpiredDataSchema,
    type: Type.Literal('confirmed_expiration'),
  }),
  Type.Object({
    info: Type.String(),
    type: Type.Literal('interrupted'),
  }),
  Type.Object({
    type: Type.Literal('new'),
  }),
]);
export type PredicateStatus = Static<typeof PredicateStatusSchema>;

export const SerializedPredicateSchema = Type.Object({
  chain: Type.Union([Type.Literal('stacks'), Type.Literal('bitcoin')]),
  uuid: Type.String(),
  network: Type.Union([Type.Literal('mainnet'), Type.Literal('testnet')]),
  predicate: Type.Any(),
  status: PredicateStatusSchema,
  enabled: Type.Boolean(),
});
export type SerializedPredicate = Static<typeof SerializedPredicateSchema>;

export const SerializedPredicateResponseSchema = Type.Union([
  Type.Object({
    status: Type.Literal(404),
  }),
  Type.Object({
    result: SerializedPredicateSchema,
    status: Type.Literal(200),
  }),
]);
export type SerializedPredicateResponse = Static<typeof SerializedPredicateResponseSchema>;
