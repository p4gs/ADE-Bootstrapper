/**
 * Module registry — the fifteen spec components in deterministic apply order
 * (spec "Core components" order). This ordering also fixes instruction-block
 * composition order, so canonical output is stable.
 *
 * Extension model: implement `AdeModule` in a new file and add it here.
 */
import type { AdeModule } from "./types.ts";
import { guardrailsModule } from "./modules/guardrails.ts";
import { supplyChainModule } from "./modules/supply-chain.ts";
import { sandboxModule } from "./modules/sandbox.ts";
import { contextMgmtModule } from "./modules/context-mgmt.ts";
import { scaffoldingModule } from "./modules/scaffolding.ts";
import { memoryModule } from "./modules/memory.ts";
import { injectionDefenseModule } from "./modules/injection-defense.ts";
import { configGovernanceModule } from "./modules/config-governance.ts";
import { observabilityModule } from "./modules/observability.ts";
import { approvalGatesModule } from "./modules/approval-gates.ts";
import { secretsModule } from "./modules/secrets.ts";
import { gitHygieneModule } from "./modules/git-hygiene.ts";
import { costGovernanceModule } from "./modules/cost-governance.ts";
import { reproducibilityModule } from "./modules/reproducibility.ts";
import { tokenEfficiencyModule } from "./modules/token-efficiency.ts";

export const MODULES: AdeModule[] = [
  guardrailsModule,
  supplyChainModule,
  sandboxModule,
  contextMgmtModule,
  scaffoldingModule,
  memoryModule,
  injectionDefenseModule,
  configGovernanceModule,
  observabilityModule,
  approvalGatesModule,
  secretsModule,
  gitHygieneModule,
  costGovernanceModule,
  reproducibilityModule,
  tokenEfficiencyModule,
];

export function moduleIds(): string[] {
  return MODULES.map((module) => module.id);
}

export function getModule(id: string): AdeModule | undefined {
  return MODULES.find((module) => module.id === id);
}
