import { Static, Type, TypeBoxTypeProvider } from '@fastify/type-provider-typebox';
import { TypeCompiler } from '@sinclair/typebox/compiler';
import Fastify, {
  FastifyInstance,
  FastifyPluginCallback,
  FastifyReply,
  FastifyRequest,
} from 'fastify';
import { Server } from 'http';
import { request } from 'undici';
import { logger, PINO_CONFIG } from './util/logger';
import { timeout } from './util/helpers';
import { Payload, PayloadSchema } from './schemas/payload';
import { PredicateHeaderSchema } from './schemas/predicate';
import { BitcoinIfThisOptionsSchema, BitcoinIfThisSchema } from './schemas/bitcoin/if_this';
import { StacksIfThisOptionsSchema, StacksIfThisSchema } from './schemas/stacks/if_this';
import { registerPredicates, removePredicates } from './predicates';

/** Function type for a Chainhook event callback */
export type OnEventCallback = (uuid: string, payload: Payload) => Promise<void>;

const ServerOptionsSchema = Type.Object({
  hostname: Type.String(),
  port: Type.Integer(),
  auth_token: Type.String(),
  external_base_url: Type.String(),

  /** Wait for the chainhook node to be available before submitting predicates */
  wait_for_chainhook_node: Type.Optional(Type.Boolean({ default: true })),
  /** Validate the JSON schema of received chainhook payloads and report errors when invalid */
  validate_chainhook_payloads: Type.Optional(Type.Boolean({ default: false })),
  /** Validate the authorization token sent by the server is correct. */
  validate_token_authorization: Type.Optional(Type.Boolean({ default: true })),
  /** Size limit for received chainhook payloads (default 40MB) */
  body_limit: Type.Optional(Type.Number({ default: 41943040 })),
  /** Node type: `chainhook` or `ordhook` */
  node_type: Type.Optional(
    Type.Union([Type.Literal('chainhook'), Type.Literal('ordhook')], {
      default: 'chainhook',
    })
  ),
  /**
   * Directory where registered predicates will be persisted to disk so they can be recalled on
   * restarts. Predicates will not be persisted if this option is left undefined.
   */
  predicates_disk_file_path: Type.Optional(Type.String()),
  /**
   * How often we should check with the Chainhook server to make sure our predicates are active and
   * up to date. If they become obsolete, we will attempt to re-register them. Interval will be
   * turned off if this option is left undefined.
   */
  predicate_health_check_interval_ms: Type.Optional(Type.Integer()),
});
/** Local event server connection and authentication options */
export type ServerOptions = Static<typeof ServerOptionsSchema>;

const ChainhookNodeOptionsSchema = Type.Object({
  base_url: Type.String(),
});
/** Chainhook node connection options */
export type ChainhookNodeOptions = Static<typeof ChainhookNodeOptionsSchema>;

/**
 * Throw this error when processing a Chainhook Payload if you believe it is a bad request. This
 * will cause the server to return a `400` status code.
 */
export class BadPayloadRequestError extends Error {
  constructor(message: string) {
    super(message);
    this.name = this.constructor.name;
  }
}

const IfThisThenNothingSchema = Type.Union([
  Type.Composite([
    BitcoinIfThisOptionsSchema,
    Type.Object({
      if_this: BitcoinIfThisSchema,
    }),
  ]),
  Type.Composite([
    StacksIfThisOptionsSchema,
    Type.Object({
      if_this: StacksIfThisSchema,
    }),
  ]),
]);
const ServerPredicateSchema = Type.Composite([
  PredicateHeaderSchema,
  Type.Object({
    networks: Type.Union([
      Type.Object({
        mainnet: IfThisThenNothingSchema,
      }),
      Type.Object({
        testnet: IfThisThenNothingSchema,
      }),
    ]),
  }),
]);
/** Chainhook predicates registerable by the local event server */
export type ServerPredicate = Static<typeof ServerPredicateSchema>;

const CompiledPayloadSchema = TypeCompiler.Compile(PayloadSchema);
export const CompiledServerPredicateSchema = TypeCompiler.Compile(ServerPredicateSchema);

/**
 * Build the Chainhook Fastify event server.
 * @param serverOpts - Server options
 * @param chainhookOpts - Chainhook node options
 * @param predicates - Predicates to register
 * @param callback - Event callback function
 * @returns Fastify instance
 */
export async function buildServer(
  serverOpts: ServerOptions,
  chainhookOpts: ChainhookNodeOptions,
  predicates: ServerPredicate[],
  callback: OnEventCallback
) {
  async function waitForNode(this: FastifyInstance) {
    logger.info(
      `ChainhookEventObserver connecting to chainhook node at ${chainhookOpts.base_url}...`
    );
    while (true) {
      try {
        await request(`${chainhookOpts.base_url}/ping`, { method: 'GET', throwOnError: true });
        break;
      } catch (error) {
        logger.error(error, 'ChainhookEventObserver chainhook node not available, retrying...');
        await timeout(1000);
      }
    }
  }

  async function isEventAuthorized(request: FastifyRequest, reply: FastifyReply) {
    if (!(serverOpts.validate_token_authorization ?? true)) return;
    const authHeader = request.headers.authorization;
    if (authHeader && authHeader === `Bearer ${serverOpts.auth_token}`) {
      return;
    }
    await reply.code(403).send();
  }

  const ChainhookEventObserver: FastifyPluginCallback<
    Record<never, never>,
    Server,
    TypeBoxTypeProvider
  > = (fastify, options, done) => {
    fastify.addHook('preHandler', isEventAuthorized);
    fastify.post('/payload', async (request, reply) => {
      if (
        (serverOpts.validate_chainhook_payloads ?? false) &&
        !CompiledPayloadSchema.Check(request.body)
      ) {
        logger.error(
          [...CompiledPayloadSchema.Errors(request.body)],
          `ChainhookEventObserver received an invalid payload`
        );
        await reply.code(422).send();
        return;
      }
      const body = request.body as Payload;
      try {
        await callback(body.chainhook.uuid, body);
        await reply.code(200).send();
      } catch (error) {
        if (error instanceof BadPayloadRequestError) {
          logger.error(error, `ChainhookEventObserver bad payload`);
          await reply.code(400).send();
        } else {
          logger.error(error, `ChainhookEventObserver error processing payload`);
          await reply.code(500).send();
        }
      }
    });
    done();
  };

  const fastify = Fastify({
    trustProxy: true,
    logger: PINO_CONFIG,
    pluginTimeout: 0, // Disable so ping can retry indefinitely
    bodyLimit: serverOpts.body_limit ?? 41943040, // 40MB default
  }).withTypeProvider<TypeBoxTypeProvider>();

  if (serverOpts.wait_for_chainhook_node ?? true) fastify.addHook('onReady', waitForNode);
  fastify.addHook('onReady', async () => registerPredicates(predicates, serverOpts, chainhookOpts));
  fastify.addHook('onClose', async () => removePredicates(predicates, serverOpts, chainhookOpts));

  await fastify.register(ChainhookEventObserver);
  return fastify;
}
