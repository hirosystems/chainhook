import * as fs from 'fs';
import * as path from 'path';
import { ChainhookNodeOptions, EventObserverOptions } from './server';
import { logger } from './util/logger';
import {
  Predicate,
  PredicateSchema,
  SerializedPredicateResponse,
  ThenThatHttpPost,
} from './schemas/predicate';
import { request } from 'undici';
import { Type } from '@fastify/type-provider-typebox';
import { TypeCompiler } from '@sinclair/typebox/compiler';
import { BitcoinIfThisOptionsSchema, BitcoinIfThisSchema } from './schemas/bitcoin/if_this';
import { StacksIfThisOptionsSchema, StacksIfThisSchema } from './schemas/stacks/if_this';
import { EventObserverPredicate } from '.';
import { randomUUID } from 'crypto';

const registeredPredicates = new Map<string, Predicate>();

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
export const EventObserverPredicateSchema = Type.Composite([
  Type.Object({
    name: Type.String(),
    version: Type.Integer(),
    chain: Type.String(),
  }),
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
const CompiledPredicateSchema = TypeCompiler.Compile(PredicateSchema);

/**
 * Looks on disk and returns a map of registered Predicates, where the key is the predicate `name`
 * as defined by the user.
 */
function recallPersistedPredicatesFromDisk(basePath: string) {
  registeredPredicates.clear();
  try {
    if (!fs.existsSync(basePath)) return registeredPredicates;
    for (const file of fs.readdirSync(basePath)) {
      if (file.endsWith('.json')) {
        const predicate = fs.readFileSync(path.join(basePath, file), 'utf-8');
        if (CompiledPredicateSchema.Check(predicate)) {
          logger.info(
            `ChainhookEventObserver recalled predicate '${predicate.name}' (${predicate.uuid}) from disk`
          );
          registeredPredicates.set(predicate.name, predicate);
        }
      }
    }
  } catch (error) {
    logger.error(error, `ChainhookEventObserver unable to retrieve persisted predicates from disk`);
    registeredPredicates.clear();
  }
  return registeredPredicates;
}

function savePredicateToDisk(basePath: string, predicate: Predicate) {
  const predicatePath = `${basePath}/predicate-${encodeURIComponent(predicate.name)}.json`;
  try {
    fs.mkdirSync(basePath, { recursive: true });
    fs.writeFileSync(predicatePath, JSON.stringify(predicate, null, 2));
    logger.info(
      `ChainhookEventObserver persisted predicate '${predicate.name}' (${predicate.uuid}) to disk`
    );
  } catch (error) {
    logger.error(
      error,
      `ChainhookEventObserver unable to persist predicate '${predicate.name}' (${predicate.uuid}) to disk`
    );
  }
}

function deletePredicateFromDisk(basePath: string, predicate: Predicate) {
  const predicatePath = `${basePath}/predicate-${encodeURIComponent(predicate.name)}.json`;
  if (fs.existsSync(predicatePath)) {
    fs.rmSync(predicatePath);
    logger.info(
      `ChainhookEventObserver deleted predicate '${predicate.name}' (${predicate.uuid}) from disk`
    );
  }
}

/** Checks the Chainhook node to see if a predicate is still valid and active */
async function isPredicateActive(
  predicate: Predicate,
  chainhook: ChainhookNodeOptions
): Promise<boolean | undefined> {
  try {
    const result = await request(`${chainhook.base_url}/v1/chainhooks/${predicate.uuid}`, {
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
      `ChainhookEventObserver unable to check if predicate '${predicate.name}' (${predicate.uuid}) is active`
    );
    return false;
  }
}

/**
 * Registers a predicate in the Chainhook server. Automatically handles pre-existing predicates
 * found on disk.
 */
async function registerPredicate(
  pendingPredicate: EventObserverPredicate,
  diskPredicates: Map<string, Predicate>,
  observer: EventObserverOptions,
  chainhook: ChainhookNodeOptions
) {
  // First check if we've already registered this predicate in the past, and if so, make sure it's
  // still active on the Chainhook server.
  if (observer.node_type === 'chainhook') {
    const diskPredicate = diskPredicates.get(pendingPredicate.name);
    if (diskPredicate) {
      switch (await isPredicateActive(diskPredicate, chainhook)) {
        case true:
          logger.debug(
            `ChainhookEventObserver predicate '${diskPredicate.name}' (${diskPredicate.uuid}) is active`
          );
          return;
        case undefined:
          logger.info(
            `ChainhookEventObserver predicate '${diskPredicate.name}' (${diskPredicate.uuid}) found on disk but not on the Chainhook server`
          );
          break;
        case false:
          logger.info(
            `ChainhookEventObserver predicate '${diskPredicate.name}' (${diskPredicate.uuid}) was being used but is now inactive, removing for re-regristration`
          );
          await removePredicate(diskPredicate, observer, chainhook);
          break;
      }
    }
  }

  logger.info(`ChainhookEventObserver registering predicate '${pendingPredicate.name}'`);
  try {
    // Add the `uuid` and `then_that` portions to the predicate.
    const thenThat: ThenThatHttpPost = {
      http_post: {
        url: `${observer.external_base_url}/payload`,
        authorization_header: `Bearer ${observer.auth_token}`,
      },
    };
    const newPredicate = pendingPredicate as Predicate;
    newPredicate.uuid = randomUUID();
    if ('mainnet' in newPredicate.networks) newPredicate.networks.mainnet.then_that = thenThat;
    if ('testnet' in newPredicate.networks) newPredicate.networks.testnet.then_that = thenThat;

    const path = observer.node_type === 'chainhook' ? `/v1/chainhooks` : `/v1/observers`;
    await request(`${chainhook.base_url}${path}`, {
      method: 'POST',
      body: JSON.stringify(newPredicate),
      headers: { 'content-type': 'application/json' },
      throwOnError: true,
    });
    logger.info(
      `ChainhookEventObserver registered '${newPredicate.name}' predicate (${newPredicate.uuid})`
    );
    savePredicateToDisk(observer.predicate_disk_file_path, newPredicate);
    registeredPredicates.set(newPredicate.name, newPredicate);
  } catch (error) {
    logger.error(error, `ChainhookEventObserver unable to register predicate`);
  }
}

/** Removes a predicate from the Chainhook server */
async function removePredicate(
  predicate: Predicate,
  observer: EventObserverOptions,
  chainhook: ChainhookNodeOptions
): Promise<void> {
  const nodeType = observer.node_type ?? 'chainhook';
  const path =
    nodeType === 'chainhook'
      ? `/v1/chainhooks/${predicate.chain}/${encodeURIComponent(predicate.uuid)}`
      : `/v1/observers/${encodeURIComponent(predicate.uuid)}`;
  try {
    await request(`${chainhook.base_url}${path}`, {
      method: 'DELETE',
      headers: { 'content-type': 'application/json' },
      throwOnError: true,
    });
    logger.info(`ChainhookEventObserver removed predicate '${predicate.name}' (${predicate.uuid})`);
    deletePredicateFromDisk(observer.predicate_disk_file_path, predicate);
  } catch (error) {
    logger.error(error, `ChainhookEventObserver unable to deregister predicate`);
  }
}

/** Registers predicates with the Chainhook server when our event observer is booting up */
export async function registerAllPredicatesOnObserverReady(
  predicates: EventObserverPredicate[],
  observer: EventObserverOptions,
  chainhook: ChainhookNodeOptions
) {
  logger.info(predicates, `ChainhookEventObserver connected to ${chainhook.base_url}`);
  if (predicates.length === 0) {
    logger.info(`ChainhookEventObserver does not have predicates to register`);
    return;
  }
  const diskPredicates = recallPersistedPredicatesFromDisk(observer.predicate_disk_file_path);
  for (const predicate of predicates)
    await registerPredicate(predicate, diskPredicates, observer, chainhook);
}

/** Removes predicates from the Chainhook server when our event observer is being closed */
export async function removeAllPredicatesOnObserverClose(
  observer: EventObserverOptions,
  chainhook: ChainhookNodeOptions
) {
  const diskPredicates = recallPersistedPredicatesFromDisk(observer.predicate_disk_file_path);
  if (diskPredicates.size === 0) {
    logger.info(`ChainhookEventObserver does not have predicates to close`);
    return;
  }
  logger.info(`ChainhookEventObserver closing predicates at ${chainhook.base_url}`);
  const removals = [...registeredPredicates.values()].map(predicate =>
    removePredicate(predicate, observer, chainhook)
  );
  await Promise.allSettled(removals);
  registeredPredicates.clear();
}

export async function predicateHealthCheck(
  predicates: EventObserverPredicate[],
  observer: EventObserverOptions,
  chainhook: ChainhookNodeOptions
): Promise<void> {
  logger.debug(`ChainhookEventObserver performing predicate health check`);
  for (const predicate of registeredPredicates.values()) {
    // This will be a no-op if the predicate is already active.
    await registerPredicate(predicate, registeredPredicates, observer, chainhook);
  }
}
