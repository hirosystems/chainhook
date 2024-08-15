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
import {
  Predicate,
  PredicateHeaderSchema,
  SerializedPredicate,
  SerializedPredicateResponse,
  ThenThatHttpPost,
} from './schemas/predicate';
import { BitcoinIfThisOptionsSchema, BitcoinIfThisSchema } from './schemas/bitcoin/if_this';
import { StacksIfThisOptionsSchema, StacksIfThisSchema } from './schemas/stacks/if_this';

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
    logger.info(`ChainhookEventObserver looking for chainhook node at ${chainhookOpts.base_url}`);
    while (true) {
      try {
        await request(`${chainhookOpts.base_url}/ping`, { method: 'GET', throwOnError: true });
        break;
      } catch (error) {
        logger.error(error, 'Chainhook node not available, retrying...');
        await timeout(1000);
      }
    }
  }

  async function isPredicateActive(predicate: ServerPredicate): Promise<boolean | undefined> {
    try {
      const result = await request(`${chainhookOpts.base_url}/v1/chainhooks/${predicate.uuid}`, {
        method: 'GET',
        headers: { accept: 'application/json' },
        throwOnError: true,
      });
      const response = (await result.body.json()) as SerializedPredicateResponse;
      if (response.status == 404) return undefined;
      if (
        response.result.enabled == false ||
        response.result.status.type == 'interrupted' ||
        response.result.status.type == 'unconfirmed_expiration' ||
        response.result.status.type == 'confirmed_expiration'
      ) {
        return false;
      }
      return true;
    } catch (error) {
      logger.error(
        error,
        `ChainhookEventObserver unable to check if predicate ${predicate.uuid} is active`
      );
      return false;
    }
  }

  async function registerAllPredicates(this: FastifyInstance) {
    logger.info(predicates, `ChainhookEventObserver connected to ${chainhookOpts.base_url}`);
    if (predicates.length === 0) {
      logger.info(`ChainhookEventObserver does not have predicates to register`);
      return;
    }
    const nodeType = serverOpts.node_type ?? 'chainhook';
    const path = nodeType === 'chainhook' ? `/v1/chainhooks` : `/v1/observers`;
    const registerUrl = `${chainhookOpts.base_url}${path}`;
    for (const predicate of predicates) {
      if (nodeType === 'chainhook') {
        switch (await isPredicateActive(predicate)) {
          case undefined:
            // Predicate doesn't exist.
            break;
          case true:
            logger.info(
              `ChainhookEventObserver predicate ${predicate.uuid} is already active, skipping registration`
            );
            continue;
          case false:
            logger.info(
              `ChainhookEventObserver predicate ${predicate.uuid} was being used but is now inactive, removing for re-regristration`
            );
            await removePredicate(predicate);
        }
      }
      logger.info(`ChainhookEventObserver registering predicate ${predicate.uuid}`);
      const thenThat: ThenThatHttpPost = {
        http_post: {
          url: `${serverOpts.external_base_url}/payload`,
          authorization_header: `Bearer ${serverOpts.auth_token}`,
        },
      };
      try {
        const body = predicate as Predicate;
        if ('mainnet' in body.networks) body.networks.mainnet.then_that = thenThat;
        if ('testnet' in body.networks) body.networks.testnet.then_that = thenThat;
        await request(registerUrl, {
          method: 'POST',
          body: JSON.stringify(body),
          headers: { 'content-type': 'application/json' },
          throwOnError: true,
        });
      } catch (error) {
        logger.error(error, `ChainhookEventObserver unable to register predicate`);
      }
    }
  }

  async function removePredicate(predicate: ServerPredicate): Promise<void> {
    const nodeType = serverOpts.node_type ?? 'chainhook';
    const path =
      nodeType === 'chainhook'
        ? `/v1/chainhooks/${predicate.chain}/${encodeURIComponent(predicate.uuid)}`
        : `/v1/observers/${encodeURIComponent(predicate.uuid)}`;
    try {
      await request(`${chainhookOpts.base_url}${path}`, {
        method: 'DELETE',
        headers: { 'content-type': 'application/json' },
        throwOnError: true,
      });
      logger.info(`ChainhookEventObserver removed predicate ${predicate.uuid}`);
    } catch (error) {
      logger.error(error, `ChainhookEventObserver unable to deregister predicate`);
    }
  }

  async function removeAllPredicates(this: FastifyInstance) {
    if (predicates.length === 0) {
      logger.info(`ChainhookEventObserver does not have predicates to close`);
      return;
    }
    logger.info(`ChainhookEventObserver closing predicates at ${chainhookOpts.base_url}`);
    const removals = predicates.map(predicate => removePredicate(predicate));
    await Promise.allSettled(removals);
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
    const compiledPayloadSchema = TypeCompiler.Compile(PayloadSchema);
    fastify.addHook('preHandler', isEventAuthorized);
    fastify.post('/payload', async (request, reply) => {
      if (
        (serverOpts.validate_chainhook_payloads ?? false) &&
        !compiledPayloadSchema.Check(request.body)
      ) {
        logger.error(
          [...compiledPayloadSchema.Errors(request.body)],
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

  if (serverOpts.wait_for_chainhook_node ?? true) {
    fastify.addHook('onReady', waitForNode);
  }
  fastify.addHook('onReady', registerAllPredicates);
  fastify.addHook('onClose', removeAllPredicates);

  await fastify.register(ChainhookEventObserver);
  return fastify;
}
