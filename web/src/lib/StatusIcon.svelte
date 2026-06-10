<script lang="ts" module>
  // Canonical issue-status color, exported for callers that color text
  // alongside the icon (e.g. IssueDetail's breadcrumb status label).
  export function statusCssColor(s: string): string {
    switch (s) {
      case "backlog": return "var(--text-faint)";
      case "todo": return "var(--text-muted)";
      case "active": return "var(--accent)";
      case "done": return "var(--success)";
      case "cancelled": return "var(--text-faint)";
      default: return "var(--text-faint)";
    }
  }
</script>

<script lang="ts">
  // Shared issue-status icon. Same role as PriorityIcon (LIF-128): one
  // canonical icon-and-color vocabulary so surfaces can't drift apart.
  // Before this existed, IssueList/IssueDetail carried duplicated
  // snippets, ModuleDetail used the smaller CircleCheck for done, and
  // IssueNew spoke a different language entirely (plain colored dots).
  //
  // Issue statuses only — module lifecycle states (planned/paused/...)
  // keep their own vocabulary in ModuleList/ModuleDetail.
  import {
    Circle, CircleDot, CircleDashed, CircleCheckBig, CircleX,
  } from "lucide-svelte";

  let { status, size = 14 }: { status: string; size?: number } = $props();
</script>

{#if status === "done"}
  <CircleCheckBig {size} class="shrink-0" style="color: {statusCssColor(status)}" />
{:else if status === "cancelled"}
  <CircleX {size} class="shrink-0" style="color: {statusCssColor(status)}" />
{:else if status === "active"}
  <CircleDot {size} class="shrink-0" style="color: {statusCssColor(status)}" />
{:else if status === "backlog"}
  <CircleDashed {size} class="shrink-0" style="color: {statusCssColor(status)}" />
{:else}
  <Circle {size} class="shrink-0" style="color: {statusCssColor(status)}" />
{/if}
