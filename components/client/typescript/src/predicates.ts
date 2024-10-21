import * as fs from 'fs';
import * as path from 'path';
import {
  ChainhookNodeOptions,
  CompiledServerPredicateSchema,
  ServerOptions,
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
  chainhookOpts: ChainhookNodeOptions
): Promise<boolean | undefined> {
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

async function registerPredicate(
  predicate: ServerPredicate,
  serverOpts: ServerOptions,
  chainhookOpts: ChainhookNodeOptions
) {
  const path = serverOpts.node_type === 'chainhook' ? `/v1/chainhooks` : `/v1/observers`;
  const registerUrl = `${chainhookOpts.base_url}${path}`;
  if (serverOpts.node_type === 'chainhook') {
    switch (await isPredicateActive(predicate, chainhookOpts)) {
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
        await removePredicate(predicate, serverOpts, chainhookOpts);
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
    logger.info(
      `ChainhookEventObserver registered '${predicate.name}' predicate (${predicate.uuid})`
    );
    if (serverOpts.predicates_disk_file_path)
      persistPredicateToDisk(serverOpts.predicates_disk_file_path, predicate);
  } catch (error) {
    logger.error(error, `ChainhookEventObserver unable to register predicate`);
  }
}

/** Registers predicates with the Chainhook server */
export async function registerAllPredicates(
  predicates: ServerPredicate[],
  serverOpts: ServerOptions,
  chainhookOpts: ChainhookNodeOptions
) {
  logger.info(predicates, `ChainhookEventObserver connected to ${chainhookOpts.base_url}`);
  let predicatesToRegister = predicates;
  if (serverOpts.predicates_disk_file_path) {
    logger.info(`ChainhookEventObserver recalling predicates from disk`);
    predicatesToRegister = recallPersistedPredicatesFromDisk(serverOpts.predicates_disk_file_path);
  }
  if (predicatesToRegister.length === 0) {
    logger.info(`ChainhookEventObserver does not have predicates to register`);
    return;
  }
  for (const predicate of predicatesToRegister) {
    await registerPredicate(predicate, serverOpts, chainhookOpts);
  }
}

async function removePredicate(
  predicate: ServerPredicate,
  serverOpts: ServerOptions,
  chainhookOpts: ChainhookNodeOptions
): Promise<void> {
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
    if (serverOpts.predicates_disk_file_path)
      removePredicateFromDisk(serverOpts.predicates_disk_file_path, predicate);
  } catch (error) {
    logger.error(error, `ChainhookEventObserver unable to deregister predicate`);
  }
}

/** Removes predicates from the Chainhook server */
export async function removeAllPredicates(
  predicates: ServerPredicate[],
  serverOpts: ServerOptions,
  chainhookOpts: ChainhookNodeOptions
) {
  if (predicates.length === 0) {
    logger.info(`ChainhookEventObserver does not have predicates to close`);
    return;
  }
  logger.info(`ChainhookEventObserver closing predicates at ${chainhookOpts.base_url}`);
  const removals = predicates.map(predicate =>
    removePredicate(predicate, serverOpts, chainhookOpts)
  );
  await Promise.allSettled(removals);
}

export async function predicateHealthCheck(
  predicates: ServerPredicate[],
  serverOpts: ServerOptions,
  chainhookOpts: ChainhookNodeOptions
): Promise<void> {
  logger.debug(`ChainhookEventObserver performing predicate health check`);
  for (const predicate of predicates) {
    await registerPredicate(predicate, serverOpts, chainhookOpts);
  }
}
