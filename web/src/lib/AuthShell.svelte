<script lang="ts">
  // LIF-194: shared auth chrome for Login + Signup. A focused, single-column
  // brand-forward experience (replaces the old split dark-panel layout):
  // a soft green brand glow, the logo + wordmark, then one elevated card
  // holding the form. Green primary action matches the app's button language.
  import ThemeToggle from "./ThemeToggle.svelte";

  let {
    title,
    subtitle,
    altText,
    altLabel,
    altHref,
    navigate,
    children,
  }: {
    title: string;
    subtitle: string;
    altText: string;
    altLabel: string;
    altHref: string;
    navigate: (path: string) => void;
    children: import("svelte").Snippet;
  } = $props();
</script>

<div class="min-h-dvh relative flex flex-col items-center justify-center px-4 py-12 bg-[var(--bg)] overflow-hidden">
  <!-- Soft brand glow (the lizard green), low and calm. -->
  <div
    aria-hidden="true"
    class="pointer-events-none absolute -top-28 left-1/2 -translate-x-1/2
           w-[560px] h-[360px] rounded-full blur-[110px] opacity-[0.16]"
    style="background: radial-gradient(closest-side, var(--success), transparent)"
  ></div>

  <div class="absolute top-4 right-4 z-10">
    <ThemeToggle />
  </div>

  <div class="relative w-full max-w-[400px] animate-reveal">
    <!-- Brand -->
    <div class="flex flex-col items-center text-center mb-7">
      <a
        href="https://github.com/VoidNullable/lific"
        target="_blank"
        rel="noopener noreferrer"
        title="View Lific on GitHub"
        class="hover:opacity-90 transition-opacity"
      >
        <img src="/logo.webp" alt="Lific" width="56" height="56" class="rounded-2xl shadow-[0_4px_16px_rgba(0,0,0,0.12)]" />
      </a>
      <h1 class="font-display text-[1.75rem] tracking-tight text-[var(--text)] leading-none mt-3.5">
        Lific
      </h1>
      <p class="text-[0.875rem] text-[var(--text-muted)] mt-1.5 max-w-[30ch]">
        Lightweight issue tracking built for AI-driven development.
      </p>
    </div>

    <!-- Card -->
    <div
      class="rounded-2xl bg-[var(--surface)] border border-[var(--border)]
             shadow-[0_8px_30px_rgba(0,0,0,0.08)] p-6 sm:p-7"
    >
      <div class="mb-5">
        <h2 class="font-display text-[1.25rem] tracking-tight text-[var(--text)] leading-none">{title}</h2>
        <p class="text-[0.875rem] text-[var(--text-muted)] mt-1.5">{subtitle}</p>
      </div>
      {@render children()}
    </div>

    <!-- Switch -->
    <p class="text-center mt-5 text-[0.875rem] text-[var(--text-muted)]">
      {altText}
      <button
        class="text-[var(--accent)] font-medium bg-transparent border-none cursor-pointer hover:underline"
        onclick={() => navigate(altHref)}
      >
        {altLabel}
      </button>
    </p>

    <!-- Footer -->
    <p class="text-center mt-8 text-[0.75rem] text-[var(--text-faint)] flex items-center justify-center gap-2">
      <span class="font-mono">v{__APP_VERSION__}</span>
      <span class="w-3 h-px bg-[var(--text-faint)]"></span>
      <span>Designed for prolific projects</span>
    </p>
  </div>
</div>
