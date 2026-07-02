<script lang="ts">
  // LIF-246 — shared skeleton primitive for content-shaped loading states.
  // Three variants (bar / block / circle) cover every loading treatment
  // in the app: text-line placeholders, card/panel placeholders, and
  // avatar/icon placeholders. Callers size it with `class` (width/height
  // utilities) — this component only owns the shimmer + shape.
  //
  // Reduced motion (lib/theme.ts `data-motion`): the shimmer sweep is a
  // CSS `background-position` animation, which the global
  // `html[data-motion="reduced"] * { animation-duration: 0.001ms }` rule
  // in app.css already collapses to a static block — no JS check needed
  // here (unlike animate:/transition: directives, which use the Web
  // Animations API and bypass that CSS rule; see IssueList's
  // animate:flip and App.svelte's route fade for the JS-driven cases
  // that DO need an explicit motionReduced() check).
  let {
    variant = "bar",
    class: className = "",
  }: {
    variant?: "bar" | "block" | "circle";
    /** Sizing utilities (width, height/size) — always required, every
     *  call site supplies its own. Rounding is optional: omit it to get
     *  the variant's default radius, or pass a `rounded-*` class to
     *  override (e.g. a `circle` used for a rounded-square icon
     *  placeholder rather than a true circle). */
    class?: string;
  } = $props();

  // This project has no tailwind-merge/clsx, so two Tailwind utility
  // classes that set the same CSS property (e.g. a baked-in
  // `rounded-full` default alongside a caller's `rounded-md` override)
  // would both land in the class list with equal specificity — the
  // winner would depend on Tailwind's stylesheet generation order, not
  // markup order, which is exactly the kind of "unpredictable until you
  // check devtools" bug this component shouldn't ship. So: no default
  // sizing classes at all (every call site already passes its own
  // w-*/h-*/size-* — that's not optional), and the default radius is
  // only applied when the caller hasn't already passed a `rounded`
  // utility of their own.
  let hasRoundedOverride = $derived(/\brounded(?:-|\b)/.test(className));
  let defaultRounded = $derived(
    hasRoundedOverride ? "" : variant === "circle" ? "rounded-full" : variant === "block" ? "rounded-lg" : "rounded",
  );
</script>

<div
  class="skeleton-shimmer bg-[var(--bg-subtle)] {defaultRounded} {className}"
  aria-hidden="true"
></div>

<style>
  /* Shimmer sweep: a lighter band travels left-to-right across the
     --bg-subtle fill via a background-position animation on a wide
     gradient — cheaper than animating opacity/filter per-frame, and GPU
     composited (background-position on a gradient is a paint-only cost,
     not layout). Reduced-motion collapses this via the global
     animation-duration override (see comment above), leaving a static
     --bg-subtle block. */
  .skeleton-shimmer {
    background-image: linear-gradient(
      100deg,
      transparent 30%,
      var(--shimmer-band, rgba(255, 255, 255, 0.35)) 50%,
      transparent 70%
    );
    background-size: 200% 100%;
    background-repeat: no-repeat;
    animation: skeleton-sweep 1.6s ease-in-out infinite;
  }

  :global(.dark) .skeleton-shimmer {
    --shimmer-band: rgba(255, 255, 255, 0.06);
  }

  @keyframes skeleton-sweep {
    0% {
      background-position: 150% 0;
    }
    100% {
      background-position: -50% 0;
    }
  }
</style>
