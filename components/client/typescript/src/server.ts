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
  registerAllPredicatesOnObserverReady,
  removeAllPredicatesOnObserverClose,
} from './predicates';
import {
  ChainhookNodeOptions,
  EventObserverOptions,
  EventObserverPredicate,
  OnPredicatePayloadCallback,
} from '.';

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

/**
 * Build the Chainhook Fastify event server.
 * @param observer - Event observer options
 * @param chainhook - Chainhook node options
 * @param predicates - Predicates to register
 * @param callback - Event callback function
 * @returns Fastify instance
 */
export async function buildServer(
  observer: EventObserverOptions,
  chainhook: ChainhookNodeOptions,
  predicates: EventObserverPredicate[],
  callback: OnPredicatePayloadCallback
) {
  async function waitForNode(this: FastifyInstance) {
    logger.info(`ChainhookEventObserver looking for chainhook node at ${chainhook.base_url}`);
    while (true) {
      try {
        await request(`${chainhook.base_url}/ping`, { method: 'GET', throwOnError: true });
        break;
      } catch (error) {
        logger.error(error, 'ChainhookEventObserver chainhook node not available, retrying...');
        await timeout(1000);
      }
    }
  }

  async function isEventAuthorized(request: FastifyRequest, reply: FastifyReply) {
    if (!(observer.validate_token_authorization ?? true)) return;
    const authHeader = request.headers.authorization;
    if (authHeader && authHeader === `Bearer ${observer.auth_token}`) {
      return;
    }
    await reply.code(403).send();
  }

  const ChainhookEventObserver: FastifyPluginCallback<
    Record<never, never>,
    Server,
    TypeBoxTypeProvider
  > = (fastify, options, done) => {
    const CompiledPayloadSchema = TypeCompiler.Compile(PayloadSchema);
    fastify.addHook('preHandler', isEventAuthorized);
    fastify.post('/payload', async (request, reply) => {
      if (
        (observer.validate_chainhook_payloads ?? false) &&
        !CompiledPayloadSchema.Check(request.body)
      ) {
        logger.error(
          [...CompiledPayloadSchema.Errors(request.body)],
          `ChainhookEventObserver received an invalid payload`
        );
        await reply.code(422).send();
        return;
      }
      try {
        await callback(request.body as Payload);
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
    bodyLimit: observer.body_limit ?? 41943040, // 40MB default
  }).withTypeProvider<TypeBoxTypeProvider>();

  if (observer.wait_for_chainhook_node ?? true) fastify.addHook('onReady', waitForNode);
  fastify.addHook('onReady', async () =>
    registerAllPredicatesOnObserverReady(predicates, observer, chainhook)
  );
  fastify.addHook('onClose', async () => removeAllPredicatesOnObserverClose(observer, chainhook));

  await fastify.register(ChainhookEventObserver);
  return fastify;
}
