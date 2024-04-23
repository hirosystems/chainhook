import { Static, Type } from '@sinclair/typebox';
import { StacksPayloadSchema } from './stacks/payload';
import { BitcoinPayloadSchema } from './bitcoin/payload';

export const PayloadSchema = Type.Union([BitcoinPayloadSchema, StacksPayloadSchema]);
export type Payload = Static<typeof PayloadSchema>;
