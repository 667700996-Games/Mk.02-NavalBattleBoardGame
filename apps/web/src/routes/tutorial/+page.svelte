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
  import { sounds } from '$lib/sound';
  import { Badge, Button, Surface } from '$lib/ui';

  const lessons = [
    {
      eyebrow: '01 / DEPLOY',
      title: '함대를 숨겨 배치하십시오',
      body: '다섯 척의 함선을 10×10 해역 안에 겹치지 않게 배치합니다. 상대에게는 좌표가 전송되지 않습니다.'
    },
    {
      eyebrow: '02 / FOG OF WAR',
      title: '적 해역은 추론으로 완성하십시오',
      body: '공격하지 않은 칸은 항상 미확인 상태입니다. 적 함선의 모양이나 남은 배치를 미리 보여주지 않습니다.'
    },
    {
      eyebrow: '03 / FIRE',
      title: '좌표를 지정하고 한 번만 발사하십시오',
      body: '각 턴에 한 칸을 공격합니다. 명중·빗나감·격침 결과는 서버가 결정하고 두 지휘관에게 같은 순서로 전달합니다.'
    },
    {
      eyebrow: '04 / TURN CLOCK',
      title: '턴 시간을 작전 자원으로 관리하십시오',
      body: '제한 시간이 끝나면 턴이 자동으로 넘어갑니다. 연속 3회 시간 초과는 패배로 처리되므로 남은 시간과 경고를 함께 확인하십시오.'
    },
    {
      eyebrow: '05 / RECOVERY',
      title: '통신이 끊겨도 작전은 복구됩니다',
      body: '잠시 오프라인이 되면 서버가 재접속 유예 시간을 제공합니다. 새로고침 후에도 같은 방과 턴을 복구하며, 중복 명령은 한 번만 처리됩니다.'
    }
  ] as const;

  let step = $state(0);
  let selected = $state('C4');
  let fired = $state(false);
  let seconds = $state(18);
  let announcement = $state('튜토리얼 1단계, 함대 배치');
  const rows = ['A', 'B', 'C', 'D', 'E'];

  onMount(() => {
    const timer = setInterval(() => {
      if (step === 3) seconds = seconds > 1 ? seconds - 1 : 20;
    }, 1_000);
    return () => clearInterval(timer);
  });

  function move(next: number) {
    step = Math.max(0, Math.min(lessons.length - 1, next));
    fired = false;
    seconds = 18;
    announcement = `튜토리얼 ${step + 1}단계, ${lessons[step].title}`;
    sounds.select();
  }

  function selectCell(label: string) {
    selected = label;
    fired = false;
    announcement = `${label} 좌표 선택`;
    sounds.targetLock();
  }

  function fire() {
    fired = true;
    announcement = `${selected} 명중. 순양함 함체 손상.`;
    sounds.hit();
  }

  async function finish() {
    preferences.update((value) => ({ ...value, tutorialCompleted: true }));
    sounds.confirm();
    await goto(resolve($session ? '/lobby' : '/'));
  }
</script>

<svelte:head>
  <title>작전 튜토리얼 · Mk.01</title>
  <meta
    name="description"
    content="Mk.01의 함선 배치, 좌표 공격, 턴 시간과 재접속 규칙을 익히는 대화형 튜토리얼"
  />
</svelte:head>

<main class="tutorial shell">
  <p class="sr-only" aria-live="polite">{announcement}</p>
  <header class="tutorial-heading">
    <div>
      <Badge tone="cyan">COMMAND ACADEMY</Badge>
      <p class="eyebrow">FIRST OPERATION / TRAINING CHANNEL</p>
      <h1 class="page-title">작전 지휘 튜토리얼</h1>
      <p>한 판을 시작하기 전에 핵심 판단과 복구 규칙을 직접 확인합니다.</p>
    </div>
    <a class="exit-link" href={resolve($session ? '/lobby' : '/')}
      ><ArrowLeft size={16} /> 나중에 계속</a
    >
  </header>

  <div class="progress" aria-label={`튜토리얼 ${step + 1}/${lessons.length}단계`}>
    {#each lessons as lesson, index}
      <button
        type="button"
        class:active={index === step}
        class:complete={index < step}
        aria-current={index === step ? 'step' : undefined}
        aria-label={`${index + 1}단계 ${lesson.title}`}
        onclick={() => move(index)}
      >
        <span>{index < step ? '✓' : index + 1}</span><i></i>
      </button>
    {/each}
  </div>

  <section class="lesson-layout" aria-labelledby="lesson-title">
    <Surface tone="elevated" padding="lg" class="lesson-copy">
      <article>
        <small>{lessons[step].eyebrow}</small>
        <h2 id="lesson-title">{lessons[step].title}</h2>
        <p>{lessons[step].body}</p>
        <div class="rule-card">
          <ShieldCheck size={18} />
          <div>
            <strong>{step === 1 ? '정보 비공개 보장' : '서버 권위 규칙'}</strong>
            <span>표시된 결과는 클라이언트가 임의로 바꾸지 못합니다.</span>
          </div>
        </div>
      </article>
    </Surface>

    <Surface tone="quiet" padding="lg" class="training-console">
      <div class="console-head">
        <span><Radio size={16} /> TRAINING SIMULATION</span>
        <Badge tone={step === 4 ? 'success' : 'warning'} pulse
          >{step === 4 ? 'LINK RESTORED' : 'LIVE'}</Badge
        >
      </div>

      {#if step === 0}
        <div class="placement-demo" aria-label="함선 배치 예시">
          <div class="mini-grid deployment-grid">
            {#each Array.from({ length: 25 }) as _, index}
              <span class:ship={[6, 7, 8, 16, 21].includes(index)}></span>
            {/each}
          </div>
          <div class="demo-readout">
            <RotateCw size={18} /><strong>배치 후 회전 가능</strong><span
              >겹침 / 경계 이탈 자동 차단</span
            >
          </div>
        </div>
      {:else if step === 1}
        <div class="fog-demo">
          <div class="mini-grid fog-grid" aria-label="미확인 적 해역">
            {#each Array.from({ length: 25 }) as _, index}
              <span class:miss={index === 6} class:hit={index === 12}></span>
            {/each}
          </div>
          <p>
            <i class="legend legend--miss"></i> 빗나감 <i class="legend legend--hit"></i> 명중
            <i class="legend"></i> 미확인
          </p>
        </div>
      {:else if step === 2}
        <div class="fire-demo">
          <div class="target-grid" aria-label="공격 좌표 선택">
            {#each rows as row}
              {#each Array.from({ length: 5 }) as _, column}
                {@const label = `${row}${column + 1}`}
                <button
                  type="button"
                  class:selected={selected === label}
                  class:resolved={fired && selected === label}
                  aria-label={`${label} ${selected === label ? '선택됨' : ''}`}
                  onclick={() => selectCell(label)}>{fired && selected === label ? '×' : ''}</button
                >
              {/each}
            {/each}
          </div>
          <Button variant="danger" onclick={fire} disabled={fired} full
            ><Crosshair size={17} /> {fired ? `${selected} 명중` : `${selected} 발사`}</Button
          >
        </div>
      {:else if step === 3}
        <div class="timer-demo">
          <div class:critical={seconds <= 5} class="timer-ring" style={`--time: ${seconds / 20}`}>
            <span>TURN</span><strong>{seconds.toString().padStart(2, '0')}</strong><small
              >SECONDS</small
            >
          </div>
          <div class="timeout-pips" aria-label="연속 시간 초과 3회 중 1회">
            <i class="used"></i><i></i><i></i>
          </div>
        </div>
      {:else}
        <div class="recovery-demo">
          <div class="signal-lines"><i></i><i></i><i></i></div>
          <Radio size={42} />
          <strong>AUTHORITATIVE STATE RESTORED</strong>
          <span>방 · 배치 · 턴 · 채팅 복구 완료</span>
          <div class="recovered"><Check size={15} /> 안전한 재접속</div>
        </div>
      {/if}
    </Surface>
  </section>

  <footer class="tutorial-actions">
    <Button variant="ghost" onclick={() => move(step - 1)} disabled={step === 0}
      ><ArrowLeft size={16} /> 이전</Button
    >
    <span>{step + 1} / {lessons.length}</span>
    {#if step < lessons.length - 1}
      <Button variant="primary" onclick={() => move(step + 1)}
        >다음 훈련 <ArrowRight size={16} /></Button
      >
    {:else}
      <Button variant="success" onclick={finish}><Check size={16} /> 훈련 완료</Button>
    {/if}
  </footer>
</main>

<style>
  .tutorial {
    padding: 56px 0 100px;
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
  .exit-link {
    display: inline-flex;
    gap: 8px;
    align-items: center;
    color: var(--ink-300);
    font: 700 11px var(--font-display);
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
  :global(.lesson-copy) > article > p {
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
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    margin-top: 20px;
  }
  .tutorial-actions > :last-child {
    justify-self: end;
  }
  .tutorial-actions > span {
    color: var(--ink-500);
    font: 700 10px var(--font-display);
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
    .tutorial-actions {
      grid-template-columns: 1fr 1fr;
    }
    .tutorial-actions > span {
      display: none;
    }
  }
  :global(html[data-motion='reduced']) .timer-ring.critical {
    animation: none;
  }
</style>
