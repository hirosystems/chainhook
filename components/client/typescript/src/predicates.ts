import * as fs from 'fs';
import * as path from 'path';
import {
  ChainhookNodeOptions,
  CompiledServerPredicateSchema,
  EventObserverOptions,
  ServerPredicate,
} from './server';
import { logger } from './util/logger';
import { Predicate, SerializedPredicateResponse, ThenThatHttpPost } from './schemas/predicate';
import { request } from 'undici';

function recallPersistedPredicatesFromDisk(basePath: string): ServerPredicate[] {
  const predicates: ServerPredicate[] = [];
  try {
    if (!fs.existsSync(basePath)) return [];
    for (const file of fs.readdirSync(basePath)) {
      if (file.endsWith('.json')) {
        const predicate = fs.readFileSync(path.join(basePath, file), 'utf-8');
        if (CompiledServerPredicateSchema.Check(predicate)) {
          logger.info(`ChainhookEventObserver recalled predicate ${predicate.uuid} from disk`);
          predicates.push(predicate as ServerPredicate);
        }
      }
    }
  } catch (error) {
    logger.error(error, `ChainhookEventObserver unable to retrieve persisted predicates from disk`);
    return [];
  }
  return predicates;
}

function persistPredicateToDisk(basePath: string, predicate: ServerPredicate) {
  const predicatePath = `${basePath}/predicate-${encodeURIComponent(predicate.uuid)}.json`;
  try {
    fs.mkdirSync(basePath, { recursive: true });
    fs.writeFileSync(predicatePath, JSON.stringify(predicate, null, 2));
    logger.info(`ChainhookEventObserver persisted predicate ${predicate.uuid} to disk`);
  } catch (error) {
    logger.error(
      error,
      `ChainhookEventObserver unable to persist predicate ${predicate.uuid} to disk`
    );
  }
}

function removePredicateFromDisk(basePath: string, predicate: ServerPredicate) {
  const predicatePath = `${basePath}/predicate-${encodeURIComponent(predicate.uuid)}.json`;
  if (fs.existsSync(predicatePath)) fs.rmSync(predicatePath);
}

/** Checks the Chainhook node to see if a predicate is still valid and active */
async function isPredicateActive(
  predicate: ServerPredicate,
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
      `ChainhookEventObserver unable to check if predicate ${predicate.uuid} is active`
    );
    return false;
  }
}

/** Registers a predicate in the Chainhook server */
async function registerPredicate(
  predicate: ServerPredicate,
  observer: EventObserverOptions,
  chainhook: ChainhookNodeOptions
) {
  const path = observer.node_type === 'chainhook' ? `/v1/chainhooks` : `/v1/observers`;
  const registerUrl = `${chainhook.base_url}${path}`;
  if (observer.node_type === 'chainhook') {
    switch (await isPredicateActive(predicate, chainhook)) {
      case true:
        logger.debug(`ChainhookEventObserver predicate ${predicate.uuid} is active`);
        return;
      case undefined:
        // Predicate doesn't exist.
        break;
      case false:
        logger.info(
          `ChainhookEventObserver predicate ${predicate.uuid} was being used but is now inactive, removing for re-regristration`
        );
        await removePredicate(predicate, observer, chainhook);
    }
  }
  logger.info(`ChainhookEventObserver registering predicate ${predicate.uuid}`);
  const thenThat: ThenThatHttpPost = {
    http_post: {
      url: `${observer.external_base_url}/payload`,
      authorization_header: `Bearer ${observer.auth_token}`,
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
    logger.info(
      `ChainhookEventObserver registered '${predicate.name}' predicate (${predicate.uuid})`
    );
    if (observer.predicates_disk_file_path)
      persistPredicateToDisk(observer.predicates_disk_file_path, predicate);
  } catch (error) {
    logger.error(error, `ChainhookEventObserver unable to register predicate`);
  }
}

/** Registers predicates with the Chainhook server when our event observer is booting up */
export async function registerAllPredicatesOnObserverReady(
  predicates: ServerPredicate[],
  observer: EventObserverOptions,
  chainhook: ChainhookNodeOptions
) {
  logger.info(predicates, `ChainhookEventObserver connected to ${chainhook.base_url}`);
  let predicatesToRegister = predicates;
  if (observer.predicates_disk_file_path) {
    logger.info(`ChainhookEventObserver recalling predicates from disk`);
    predicatesToRegister = recallPersistedPredicatesFromDisk(observer.predicates_disk_file_path);
  }
  if (predicatesToRegister.length === 0) {
    logger.info(`ChainhookEventObserver does not have predicates to register`);
    return;
  }
  for (const predicate of predicatesToRegister) {
    await registerPredicate(predicate, observer, chainhook);
  }
}

/** Removes a predicate from the Chainhook server */
async function removePredicate(
  predicate: ServerPredicate,
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
    logger.info(`ChainhookEventObserver removed predicate ${predicate.uuid}`);
    if (observer.predicates_disk_file_path)
      removePredicateFromDisk(observer.predicates_disk_file_path, predicate);
  } catch (error) {
    logger.error(error, `ChainhookEventObserver unable to deregister predicate`);
  }
}

/** Removes predicates from the Chainhook server when our event observer is being closed */
export async function removeAllPredicatesOnObserverClose(
  predicates: ServerPredicate[],
  observer: EventObserverOptions,
  chainhook: ChainhookNodeOptions
) {
  if (predicates.length === 0) {
    logger.info(`ChainhookEventObserver does not have predicates to close`);
    return;
  }
  logger.info(`ChainhookEventObserver closing predicates at ${chainhook.base_url}`);
  const removals = predicates.map(predicate => removePredicate(predicate, observer, chainhook));
  await Promise.allSettled(removals);
}

export async function predicateHealthCheck(
  predicates: ServerPredicate[],
  observer: EventObserverOptions,
  chainhook: ChainhookNodeOptions
): Promise<void> {
  logger.debug(`ChainhookEventObserver performing predicate health check`);
  for (const predicate of predicates) {
    // This will be a no-op if the predicate is already active.
    await registerPredicate(predicate, observer, chainhook);
  }
}
