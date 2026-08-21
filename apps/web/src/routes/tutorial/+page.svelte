<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import {
    ArrowLeft,
    ArrowRight,
    Check,
    Crosshair,
    Radio,
    RotateCw,
    ShieldCheck
  } from '@lucide/svelte';
  import { preferences, session } from '$lib/stores';
  import { trackFunnelAbandoned, trackFunnelReached } from '$lib/funnel';
  import { sounds } from '$lib/sound';
  import { Badge, Button, Surface } from '$lib/ui';
  import { formatNumber, t, type MessageKey } from '$lib/i18n';

  const lessons: ReadonlyArray<{
    eyebrow: MessageKey;
    title: MessageKey;
    body: MessageKey;
  }> = [
    {
      eyebrow: 'tutorial.lesson1Eyebrow',
      title: 'tutorial.lesson1Title',
      body: 'tutorial.lesson1Body'
    },
    {
      eyebrow: 'tutorial.lesson2Eyebrow',
      title: 'tutorial.lesson2Title',
      body: 'tutorial.lesson2Body'
    },
    {
      eyebrow: 'tutorial.lesson3Eyebrow',
      title: 'tutorial.lesson3Title',
      body: 'tutorial.lesson3Body'
    },
    {
      eyebrow: 'tutorial.lesson4Eyebrow',
      title: 'tutorial.lesson4Title',
      body: 'tutorial.lesson4Body'
    },
    {
      eyebrow: 'tutorial.lesson5Eyebrow',
      title: 'tutorial.lesson5Title',
      body: 'tutorial.lesson5Body'
    }
  ] as const;

  let step = $state(0);
  let selected = $state('C4');
  let fired = $state(false);
  let seconds = $state(18);
  let announcement = $state(
    $t('tutorial.announcementStep', {
      step: formatNumber(1),
      title: $t(lessons[0].title)
    })
  );
  const rows = ['A', 'B', 'C', 'D', 'E'];

  onMount(() => {
    trackFunnelReached('tutorial_started');
    const timer = setInterval(() => {
      if (step === 3) seconds = seconds > 1 ? seconds - 1 : 20;
    }, 1_000);
    return () => clearInterval(timer);
  });

  function move(next: number) {
    step = Math.max(0, Math.min(lessons.length - 1, next));
    fired = false;
    seconds = 18;
    announcement = $t('tutorial.announcementStep', {
      step: formatNumber(step + 1),
      title: $t(lessons[step].title)
    });
    sounds.select();
  }

  function selectCell(label: string) {
    selected = label;
    fired = false;
    announcement = $t('tutorial.announcementCoordinate', { coordinate: label });
    sounds.targetLock();
  }

  function fire() {
    fired = true;
    announcement = $t('tutorial.announcementHit', { coordinate: selected });
    sounds.hit();
  }

  async function finish() {
    preferences.update((value) => ({ ...value, tutorialCompleted: true }));
    trackFunnelReached('tutorial_completed');
    sounds.confirm();
    await goto(resolve($session ? '/play' : '/'));
  }

  async function leaveTutorial() {
    trackFunnelAbandoned('tutorial_started');
    await goto(resolve($session ? '/play' : '/'));
  }
</script>

<svelte:head>
  <title>{$t('tutorial.metaTitle')}</title>
  <meta name="description" content={$t('tutorial.metaDescription')} />
</svelte:head>

<main class="tutorial shell">
  <p class="sr-only" aria-live="polite">{announcement}</p>
  <nav class="tutorial-mode-nav" aria-label={$t('tutorial.navigation')}>
    <Button variant="secondary" onclick={leaveTutorial}>
      <ArrowLeft size={17} />
      {$t('tutorial.changeMode')}
    </Button>
  </nav>

  <div class="tutorial-actions">
    <Button variant="secondary" onclick={() => move(step - 1)} disabled={step === 0}
      ><ArrowLeft size={16} /> {$t('tutorial.previous')}</Button
    >
    {#if step < lessons.length - 1}
      <Button variant="primary" onclick={() => move(step + 1)}
        >{$t('tutorial.next')} <ArrowRight size={16} /></Button
      >
    {:else}
      <Button variant="success" onclick={finish}
        ><Check size={16} /> {$t('tutorial.complete')}</Button
      >
    {/if}
  </div>

  <header class="tutorial-heading">
    <div>
      <Badge tone="cyan">{$t('tutorial.academy')}</Badge>
      <p class="eyebrow">{$t('tutorial.trainingChannel')}</p>
      <h1 class="page-title">{$t('tutorial.title')}</h1>
    </div>
  </header>

  <div
    class="progress"
    aria-label={$t('tutorial.progress', {
      step: formatNumber(step + 1),
      total: formatNumber(lessons.length)
    })}
  >
    {#each lessons as lesson, index (lesson.eyebrow)}
      <button
        type="button"
        class:active={index === step}
        class:complete={index < step}
        aria-current={index === step ? 'step' : undefined}
        aria-label={$t('tutorial.stepLabel', {
          step: formatNumber(index + 1),
          title: $t(lesson.title)
        })}
        onclick={() => move(index)}
      >
        <span>{index < step ? '✓' : index + 1}</span><i></i>
      </button>
    {/each}
  </div>

  <section class="lesson-layout" aria-labelledby="lesson-title">
    <Surface tone="elevated" padding="lg" class="lesson-copy">
      <article>
        <small>{$t(lessons[step].eyebrow)}</small>
        <h2 id="lesson-title">{$t(lessons[step].title)}</h2>
        <p class="lesson-body">{$t(lessons[step].body)}</p>
        <div class="rule-card">
          <ShieldCheck size={18} />
          <div>
            <strong
              >{step === 1
                ? $t('tutorial.privacyGuarantee')
                : $t('tutorial.serverAuthority')}</strong
            >
            <span>{$t('tutorial.authorityDescription')}</span>
          </div>
        </div>
      </article>
    </Surface>

    <Surface tone="quiet" padding="lg" class="training-console">
      <div class="console-head">
        <span><Radio size={16} /> {$t('tutorial.simulation')}</span>
        <Badge tone={step === 4 ? 'success' : 'warning'} pulse
          >{step === 4 ? $t('tutorial.linkRestored') : $t('tutorial.live')}</Badge
        >
      </div>

      {#if step === 0}
        <div class="placement-demo" aria-label={$t('tutorial.deploymentExample')}>
          <div class="mini-grid deployment-grid">
            {#each Array.from({ length: 25 }) as _, index (index)}
              <span class:ship={[6, 7, 8, 16, 21].includes(index)}></span>
            {/each}
          </div>
          <div class="demo-readout">
            <RotateCw size={18} /><strong>{$t('tutorial.rotateAfter')}</strong><span
              >{$t('tutorial.invalidBlocked')}</span
            >
          </div>
        </div>
      {:else if step === 1}
        <div class="fog-demo">
          <div class="mini-grid fog-grid" aria-label={$t('tutorial.unknownEnemyWaters')}>
            {#each Array.from({ length: 25 }) as _, index (index)}
              <span class:miss={index === 6} class:hit={index === 12}></span>
            {/each}
          </div>
          <p>
            <i class="legend legend--miss"></i>
            {$t('board.miss')}
            <i class="legend legend--hit"></i>
            {$t('board.hit')}
            <i class="legend"></i>
            {$t('tutorial.unknown')}
          </p>
        </div>
      {:else if step === 2}
        <div class="fire-demo">
          <div class="target-grid" aria-label={$t('tutorial.selectAttack')}>
            {#each rows as row (row)}
              {#each Array.from({ length: 5 }) as _, column (column)}
                {@const label = `${row}${column + 1}`}
                <button
                  type="button"
                  class:selected={selected === label}
                  class:resolved={fired && selected === label}
                  aria-label={`${label} ${selected === label ? $t('tutorial.selected') : ''}`}
                  onclick={() => selectCell(label)}>{fired && selected === label ? '×' : ''}</button
                >
              {/each}
            {/each}
          </div>
          <Button variant="danger" onclick={fire} disabled={fired} full
            ><Crosshair size={17} />
            {fired
              ? $t('tutorial.hit', { coordinate: selected })
              : $t('tutorial.fire', { coordinate: selected })}</Button
          >
        </div>
      {:else if step === 3}
        <div class="timer-demo">
          <div class:critical={seconds <= 5} class="timer-ring" style={`--time: ${seconds / 20}`}>
            <span>{$t('tutorial.turn')}</span><strong>{seconds.toString().padStart(2, '0')}</strong
            ><small>{$t('tutorial.seconds')}</small>
          </div>
          <div
            class="timeout-pips"
            aria-label={$t('tutorial.timeoutPips', {
              total: formatNumber(3),
              used: formatNumber(1)
            })}
          >
            <i class="used"></i><i></i><i></i>
          </div>
        </div>
      {:else}
        <div class="recovery-demo">
          <div class="signal-lines"><i></i><i></i><i></i></div>
          <Radio size={42} />
          <strong>{$t('tutorial.stateRestored')}</strong>
          <span>{$t('tutorial.recoveryScope')}</span>
          <div class="recovered"><Check size={15} /> {$t('tutorial.safeReconnect')}</div>
        </div>
      {/if}
    </Surface>
  </section>
</main>

<style>
  .tutorial {
    padding: 42px 0 100px;
  }
  .tutorial-mode-nav {
    margin-bottom: 14px;
  }
  .tutorial-heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 32px;
  }
  .tutorial-heading h1 {
    margin: 10px 0 8px;
  }
  .tutorial-heading p:last-child {
    max-width: 680px;
    color: var(--ink-300);
  }
  .progress {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    margin: 42px 0 22px;
  }
  .progress button {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 10px;
    padding: 0;
    border: 0;
    color: var(--ink-500);
    background: transparent;
    cursor: pointer;
  }
  .progress button span {
    display: grid;
    width: 28px;
    height: 28px;
    place-items: center;
    border: 1px solid var(--line);
    border-radius: 50%;
    font: 700 10px var(--font-display);
  }
  .progress button i {
    height: 1px;
    background: var(--line);
  }
  .progress button.active span,
  .progress button.complete span {
    border-color: var(--cyan-400);
    color: var(--navy-950);
    background: var(--cyan-400);
    box-shadow: 0 0 20px rgba(40, 223, 232, 0.22);
  }
  .progress button.complete i {
    background: var(--cyan-500);
  }
  .lesson-layout {
    display: grid;
    grid-template-columns: minmax(300px, 0.72fr) minmax(480px, 1.28fr);
    gap: 18px;
    min-height: 500px;
  }
  :global(.lesson-copy),
  :global(.training-console) {
    min-height: 500px;
  }
  :global(.lesson-copy) article {
    display: flex;
    min-height: 426px;
    flex-direction: column;
  }
  :global(.lesson-copy) small {
    color: var(--cyan-300);
    font: 700 10px var(--font-display);
    letter-spacing: 0.16em;
  }
  :global(.lesson-copy) h2 {
    margin: 20px 0 18px;
    font-size: clamp(28px, 3vw, 44px);
    line-height: 1.16;
    word-break: keep-all;
  }
  .lesson-body {
    color: var(--ink-200);
    line-height: 1.9;
    word-break: keep-all;
  }
  .rule-card {
    display: flex;
    gap: 12px;
    margin-top: auto;
    padding: 16px;
    border: 1px solid rgba(79, 226, 173, 0.22);
    background: rgba(79, 226, 173, 0.05);
  }
  .rule-card :global(svg) {
    flex: none;
    color: var(--green-400);
  }
  .rule-card div {
    display: grid;
    gap: 5px;
  }
  .rule-card strong {
    color: var(--green-400);
    font-size: 12px;
  }
  .rule-card span {
    color: var(--ink-300);
    font-size: 10px;
    line-height: 1.6;
  }
  :global(.training-console) {
    overflow: hidden;
    background:
      radial-gradient(circle at 50% 46%, rgba(40, 223, 232, 0.11), transparent 50%),
      rgba(2, 12, 18, 0.9);
  }
  .console-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 28px;
  }
  .console-head > span {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--ink-400);
    font: 700 9px var(--font-display);
    letter-spacing: 0.13em;
  }
  .placement-demo,
  .fog-demo,
  .fire-demo,
  .timer-demo,
  .recovery-demo {
    display: grid;
    min-height: 374px;
    place-content: center;
  }
  .mini-grid,
  .target-grid {
    display: grid;
    grid-template-columns: repeat(5, 48px);
    gap: 4px;
  }
  .mini-grid span,
  .target-grid button {
    width: 48px;
    height: 48px;
    border: 1px solid rgba(83, 233, 232, 0.16);
    background: rgba(9, 34, 45, 0.75);
  }
  .deployment-grid span.ship {
    border-color: var(--cyan-300);
    background: linear-gradient(135deg, rgba(83, 233, 232, 0.55), rgba(35, 124, 233, 0.3));
    box-shadow: inset 0 0 12px rgba(183, 253, 255, 0.15);
  }
  .demo-readout {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 2px 10px;
    margin-top: 22px;
    color: var(--cyan-300);
  }
  .demo-readout :global(svg) {
    grid-row: span 2;
  }
  .demo-readout strong {
    font-size: 12px;
  }
  .demo-readout span {
    color: var(--ink-400);
    font-size: 9px;
  }
  .fog-demo p {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    color: var(--ink-400);
    font-size: 9px;
  }
  .fog-grid .miss::after,
  .target-grid button.resolved::after {
    color: var(--ink-300);
    content: '•';
    font-size: 28px;
  }
  .fog-grid .hit,
  .target-grid button.resolved {
    border-color: var(--red-400);
    background: rgba(240, 72, 94, 0.2);
    box-shadow: 0 0 20px rgba(240, 72, 94, 0.18);
  }
  .fog-grid .hit::after {
    color: var(--red-400);
    content: '×';
    font-size: 24px;
  }
  .legend {
    display: inline-block;
    width: 9px;
    height: 9px;
    border: 1px solid var(--line);
  }
  .legend--miss {
    border-radius: 50%;
    background: var(--ink-300);
  }
  .legend--hit {
    border-color: var(--red-400);
    background: var(--red-500);
  }
  .fire-demo {
    gap: 20px;
  }
  .target-grid button {
    position: relative;
    display: grid;
    place-items: center;
    color: var(--red-400);
    font-size: 24px;
    cursor: crosshair;
  }
  .target-grid button:hover,
  .target-grid button:focus-visible,
  .target-grid button.selected {
    border-color: var(--cyan-300);
    background: rgba(40, 223, 232, 0.12);
    box-shadow: inset 0 0 14px rgba(40, 223, 232, 0.12);
  }
  .timer-demo {
    justify-items: center;
    gap: 24px;
  }
  .timer-ring {
    display: grid;
    width: 210px;
    height: 210px;
    place-content: center;
    border: 2px solid var(--cyan-400);
    border-radius: 50%;
    text-align: center;
    box-shadow:
      0 0 44px rgba(40, 223, 232, 0.12),
      inset 0 0 35px rgba(40, 223, 232, 0.08);
  }
  .timer-ring span,
  .timer-ring small {
    color: var(--ink-400);
    font: 700 9px var(--font-display);
    letter-spacing: 0.18em;
  }
  .timer-ring strong {
    color: var(--cyan-200);
    font: 700 78px/1 var(--font-display);
  }
  .timer-ring.critical {
    border-color: var(--red-400);
    animation: pulse-danger 0.8s ease-in-out infinite alternate;
  }
  .timeout-pips {
    display: flex;
    gap: 8px;
  }
  .timeout-pips i {
    width: 36px;
    height: 5px;
    background: var(--line);
  }
  .timeout-pips i.used {
    background: var(--amber-400);
  }
  .recovery-demo {
    justify-items: center;
    color: var(--cyan-300);
    text-align: center;
  }
  .recovery-demo > strong {
    margin-top: 22px;
    font: 700 17px var(--font-display);
    letter-spacing: 0.08em;
  }
  .recovery-demo > span {
    margin-top: 6px;
    color: var(--ink-300);
    font-size: 10px;
  }
  .signal-lines {
    display: flex;
    gap: 5px;
    margin-bottom: 14px;
    align-items: end;
  }
  .signal-lines i {
    width: 5px;
    height: 12px;
    background: var(--cyan-400);
  }
  .signal-lines i:nth-child(2) {
    height: 22px;
  }
  .signal-lines i:nth-child(3) {
    height: 34px;
  }
  .recovered {
    display: flex;
    gap: 7px;
    align-items: center;
    margin-top: 30px;
    padding: 8px 13px;
    border: 1px solid rgba(79, 226, 173, 0.3);
    color: var(--green-400);
    font-size: 10px;
  }
  .tutorial-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    margin-bottom: 32px;
  }
  @keyframes pulse-danger {
    to {
      box-shadow:
        0 0 52px rgba(240, 72, 94, 0.24),
        inset 0 0 35px rgba(240, 72, 94, 0.12);
    }
  }
  @media (max-width: 850px) {
    .tutorial {
      padding-top: 36px;
    }
    .tutorial-heading {
      align-items: start;
      flex-direction: column;
    }
    .lesson-layout {
      grid-template-columns: 1fr;
    }
    :global(.lesson-copy),
    :global(.training-console) {
      min-height: auto;
    }
    :global(.lesson-copy) article {
      min-height: 360px;
    }
  }
  @media (max-width: 540px) {
    .progress button {
      gap: 4px;
    }
    .progress button i {
      display: none;
    }
    .progress {
      justify-items: center;
    }
    .mini-grid,
    .target-grid {
      grid-template-columns: repeat(5, 42px);
    }
    .mini-grid span,
    .target-grid button {
      width: 42px;
      height: 42px;
    }
  }
  :global(html[data-motion='reduced']) .timer-ring.critical {
    animation: none;
  }
</style>
