<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import {
    ArrowRight,
    Crosshair,
    KeyRound,
    LockKeyhole,
    Radio,
    RotateCw,
    ShieldCheck,
    Waves
  } from '@lucide/svelte';
  import { api } from '$lib/api';
  import { trackFunnelFailure, trackFunnelReached } from '$lib/funnel';
  import { localizeError, t } from '$lib/i18n';
  import { session } from '$lib/stores';
  import { Badge, Button, Field, Surface } from '$lib/ui';

  let nickname = '';
  let submitting = false;
  let error = '';
  let existingSession = false;
  let accountLogin = false;
  let accountId = '';
  let recoveryKey = '';

  onMount(async () => {
    try {
      const current = await api.currentSession();
      session.set(current);
      nickname = current.nickname;
      existingSession = true;
    } catch {
      existingSession = false;
      trackFunnelReached('landing');
    }
  });

  async function enterLobby() {
    error = '';
    if (existingSession) {
      await goto(resolve('/lobby'));
      return;
    }
    submitting = true;
    try {
      const created = await api.createSession(nickname);
      session.set(created);
      trackFunnelReached('session_created');
      await goto(resolve('/lobby'));
    } catch (caught) {
      trackFunnelFailure('session_created', 'session_creation');
      error = localizeError(caught, 'landing.sessionCreateError');
    } finally {
      submitting = false;
    }
  }

  async function signInAccount() {
    submitting = true;
    error = '';
    try {
      const authenticated = await api.loginAccount(accountId.trim(), recoveryKey.trim());
      session.set(authenticated);
      await goto(resolve('/lobby'));
    } catch (caught) {
      error = localizeError(caught, 'landing.accountLoginError');
    } finally {
      submitting = false;
    }
  }
</script>

<svelte:head><title>{$t('landing.meta.title')}</title></svelte:head>

<section class="hero shell">
  <div class="hero__copy">
    <div class="hero__status-line">
      <Badge tone="success" pulse>{$t('landing.liveNetwork')}</Badge>
    </div>

    <p class="eyebrow">{$t('landing.eyebrow')}</p>
    <h1 class="display-title">
      <span class="display-title__line">{$t('landing.titleLineOne')}</span>
      <span class="display-title__line display-title__line--signal"
        >{$t('landing.titleLineTwo')}</span
      >
    </h1>
    <Surface tone="elevated" padding="md" class="command-entry">
      {#if accountLogin && !existingSession}
        <form
          onsubmit={(event) => {
            event.preventDefault();
            signInAccount();
          }}
        >
          <div class="command-entry__head">
            <div class="command-symbol"><KeyRound size={17} /></div>
            <div>
              <small>{$t('landing.accountRecoveryChannel')}</small><strong
                >{$t('landing.accountIdentity')}</strong
              >
            </div>
            <span>{$t('landing.verified')}</span>
          </div>
          <div class="account-entry-fields">
            <Field
              id="account-id"
              label={$t('landing.accountId')}
              bind:value={accountId}
              autocomplete="username"
              placeholder={$t('landing.accountIdPlaceholder')}
              disabled={submitting}
              required
            />
            <Field
              id="recovery-key"
              label={$t('landing.recoveryKey')}
              bind:value={recoveryKey}
              autocomplete="current-password"
              placeholder={$t('landing.recoveryKeyPlaceholder')}
              minlength={43}
              maxlength={43}
              disabled={submitting}
              {error}
              required
            />
          </div>
          <div class="account-entry-actions">
            <button type="button" class="entry-switch" onclick={() => (accountLogin = false)}
              >{$t('landing.startAsGuest')}</button
            >
            <Button variant="primary" size="lg" type="submit" loading={submitting}
              >{$t('landing.returnWithAccount')} <ArrowRight size={18} /></Button
            >
          </div>
        </form>
      {:else}
        <form
          onsubmit={(event) => {
            event.preventDefault();
            enterLobby();
          }}
        >
          <div class="command-entry__head">
            <div class="command-symbol"><Crosshair size={17} /></div>
            <div>
              <small>{$t('landing.commanderAuthorization')}</small>
              <strong
                >{existingSession
                  ? $t('landing.sessionVerified')
                  : $t('landing.registerCallsign')}</strong
              >
            </div>
            <span>{existingSession ? $t('landing.resume') : $t('landing.guestAccess')}</span>
          </div>
          <div class="command-entry__controls">
            <Field
              id="nickname"
              label={$t('landing.nickname')}
              bind:value={nickname}
              minlength={2}
              maxlength={16}
              autocomplete="nickname"
              placeholder={$t('landing.nicknamePlaceholder')}
              disabled={existingSession || submitting}
              {error}
              required
            />
            <Button variant="primary" size="lg" type="submit" loading={submitting}>
              {existingSession ? $t('landing.resumeOperation') : $t('landing.enterLobby')}
              <ArrowRight size={18} />
            </Button>
          </div>
          {#if !existingSession}<button
              type="button"
              class="entry-switch"
              onclick={() => (accountLogin = true)}
              ><KeyRound size={13} /> {$t('landing.useExistingAccount')}</button
            >{/if}
        </form>
      {/if}
    </Surface>

    <div class="trust-row" aria-label={$t('landing.securityFeatures')}>
      <span
        ><ShieldCheck size={15} /><strong>{$t('landing.serverAuthority')}</strong><small
          >{$t('landing.serverAuthorityCode')}</small
        ></span
      >
      <span
        ><LockKeyhole size={15} /><strong>{$t('landing.privateCoordinates')}</strong><small
          >{$t('landing.privateCoordinatesCode')}</small
        ></span
      >
      <span
        ><Radio size={15} /><strong>{$t('landing.realtimeReconnect')}</strong><small
          >{$t('landing.realtimeReconnectCode')}</small
        ></span
      >
    </div>
    <a class="tutorial-link" href={resolve('/tutorial')}
      >{$t('landing.tutorialLink')} <ArrowRight size={18} /></a
    >
  </div>

  <div class="command-visual" aria-hidden="true">
    <div class="visual-coordinates visual-coordinates--top">43°37' N / 128°28' E</div>
    <div class="visual-coordinates visual-coordinates--side">SECTOR 07-N</div>
    <div class="radar-shell">
      <div class="radar-horizon"></div>
      <div class="radar-grid"></div>
      <div class="radar-ticks"></div>
      <div class="radar-sector radar-sector--one"></div>
      <div class="radar-sector radar-sector--two"></div>
      <div class="radar-ring radar-ring--one"></div>
      <div class="radar-ring radar-ring--two"></div>
      <div class="radar-ring radar-ring--three"></div>
      <div class="radar-cross radar-cross--x"></div>
      <div class="radar-cross radar-cross--y"></div>
      <div class="radar-sweep"></div>
      <div class="radar-origin"><Crosshair size={26} strokeWidth={1} /></div>
      <div class="radar-bearing radar-bearing--north">000°</div>
      <div class="radar-bearing radar-bearing--east">090°</div>
      <div class="radar-bearing radar-bearing--south">180°</div>
      <div class="radar-bearing radar-bearing--west">270°</div>
      <div class="radar-range radar-range--outer">025 NM</div>
      <div class="radar-range radar-range--inner">010</div>
      <span class="contact contact--one contact--hostile"
        ><i></i><span><em>TGT-04</em><small>046° / 18.2 NM</small></span></span
      >
      <span class="contact contact--two contact--friendly"
        ><i></i><span><em>FRD-02</em><small>211° / 11.7 NM</small></span></span
      >
      <span class="contact contact--three contact--unknown"
        ><i></i><span><em>UNK-12</em><small>084° / 21.5 NM</small></span></span
      >
      <div class="fleet-trace fleet-trace--one"><i></i><i></i><i></i><i></i><i></i></div>
      <div class="fleet-trace fleet-trace--two"><i></i><i></i><i></i></div>
      <div class="radar-readout">
        <span>{$t('landing.passiveArray')}</span><strong
          >{$t('landing.noise', { value: '02.8' })}</strong
        >
      </div>
    </div>
    <div class="telemetry-card telemetry-card--top">
      <small>{$t('landing.sonarArray')}</small><strong>{$t('landing.active')}</strong><span
        >{$t('landing.scanRate', { value: '04.8s' })}</span
      >
    </div>
    <div class="telemetry-card telemetry-card--bottom">
      <small>{$t('landing.oceanDepth')}</small><strong
        >{$t('landing.depthValue', { value: '4,218' })}</strong
      ><span>{$t('landing.thermalStable')}</span>
    </div>
    <div class="visual-index"><span>01</span><i></i><span>04</span></div>
  </div>
</section>

<section class="mission-brief shell" aria-labelledby="mission-title">
  <header class="mission-brief__heading">
    <div>
      <p class="eyebrow">{$t('landing.missionProtocol')}</p>
      <h2 id="mission-title">{$t('landing.missionTitle')}</h2>
    </div>
    <p>{$t('landing.missionLead')}</p>
  </header>
  <div class="mission-grid">
    <Surface tone="interactive" padding="lg">
      <article>
        <span class="mission-number">01</span><Waves size={22} /><small
          >{$t('landing.deployCode')}</small
        >
        <h3>{$t('landing.deployTitle')}</h3>
        <p>{$t('landing.deployDescription')}</p>
      </article>
    </Surface>
    <Surface tone="interactive" padding="lg">
      <article>
        <span class="mission-number">02</span><Crosshair size={22} /><small
          >{$t('landing.deduceCode')}</small
        >
        <h3>{$t('landing.deduceTitle')}</h3>
        <p>{$t('landing.deduceDescription')}</p>
      </article>
    </Surface>
    <Surface tone="interactive" padding="lg">
      <article>
        <span class="mission-number">03</span><RotateCw size={22} /><small
          >{$t('landing.endureCode')}</small
        >
        <h3>{$t('landing.endureTitle')}</h3>
        <p>{$t('landing.endureDescription')}</p>
      </article>
    </Surface>
  </div>
</section>

<style>
  .hero {
    display: grid;
    grid-template-columns: minmax(0, 1.04fr) minmax(440px, 0.96fr);
    gap: clamp(48px, 7vw, 112px);
    align-items: center;
    min-height: min(890px, calc(100vh - 72px));
    padding-block: 64px 72px;
  }
  .hero__copy {
    position: relative;
    z-index: 2;
  }
  .hero__status-line {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 42px;
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.12em;
  }
  .display-title {
    max-width: 830px;
  }
  .display-title__line {
    display: block;
    white-space: nowrap;
  }
  .display-title__line--signal {
    color: transparent;
    background: linear-gradient(110deg, #ecffff 8%, #74f7f7 48%, #2ba6e9 96%);
    background-clip: text;
    filter: drop-shadow(0 10px 32px rgba(20, 198, 213, 0.1));
  }
  :global(.command-entry) {
    max-width: 720px;
    margin-top: 28px;
  }
  :global(.command-entry) form {
    display: grid;
    gap: 20px;
  }
  .command-entry__head {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 12px;
    padding-bottom: 16px;
    border-bottom: 1px solid var(--line);
  }
  .command-symbol {
    display: grid;
    width: 38px;
    height: 38px;
    place-items: center;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    color: var(--cyan-300);
    background: rgba(40, 223, 232, 0.06);
  }
  .command-entry__head > div:nth-child(2) {
    display: grid;
    gap: 3px;
  }
  .command-entry__head small,
  .command-entry__head > span {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.14em;
  }
  .command-entry__head strong {
    font-size: 12px;
  }
  .command-entry__head > span {
    color: var(--green-400);
  }
  .command-entry__controls {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: end;
    gap: 12px;
  }
  .account-entry-fields {
    display: grid;
    grid-template-columns: 1fr 1.45fr;
    gap: 10px;
  }
  .account-entry-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .entry-switch {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    width: fit-content;
    padding: 0;
    border: 0;
    color: var(--cyan-300);
    background: transparent;
    font: 700 9px var(--font-display);
    cursor: pointer;
  }
  .entry-switch:hover,
  .entry-switch:focus-visible {
    color: white;
  }
  .trust-row {
    display: flex;
    flex-wrap: wrap;
    gap: 24px;
    margin-top: 24px;
  }
  .trust-row span {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 1px 7px;
    color: var(--cyan-400);
  }
  .trust-row :global(svg) {
    grid-row: 1 / 3;
  }
  .trust-row strong {
    color: var(--ink-300);
    font-size: 10px;
  }
  .trust-row small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.12em;
  }
  .tutorial-link {
    display: flex;
    gap: 10px;
    align-items: center;
    width: fit-content;
    min-height: 44px;
    margin: 14px 0 0 auto;
    padding-left: 12px;
    color: var(--cyan-300);
    font: 700 clamp(13px, 1.1vw, 16px) var(--font-display);
    letter-spacing: 0.04em;
  }
  .tutorial-link:hover {
    color: var(--cyan-200);
  }
  .command-visual {
    position: relative;
    display: grid;
    place-items: center;
    aspect-ratio: 1;
  }
  .command-visual::before {
    position: absolute;
    inset: 7%;
    content: '';
    border-radius: 50%;
    background: radial-gradient(circle, rgba(15, 154, 178, 0.15), transparent 68%);
    filter: blur(26px);
  }
  .radar-shell {
    position: relative;
    width: min(100%, 600px);
    aspect-ratio: 1;
    overflow: hidden;
    border: 1px solid rgba(79, 222, 231, 0.28);
    border-radius: 50%;
    background: radial-gradient(circle, rgba(10, 65, 80, 0.46), rgba(2, 13, 20, 0.94) 67%);
    box-shadow:
      inset 0 0 100px rgba(22, 186, 204, 0.07),
      0 0 100px rgba(3, 123, 150, 0.08);
  }
  .radar-shell::after {
    position: absolute;
    inset: 4%;
    content: '';
    border: 1px solid rgba(57, 214, 226, 0.08);
    border-radius: 50%;
  }
  .radar-grid {
    position: absolute;
    inset: 0;
    opacity: 0.38;
    background-image:
      linear-gradient(rgba(54, 190, 208, 0.1) 1px, transparent 1px),
      linear-gradient(90deg, rgba(54, 190, 208, 0.1) 1px, transparent 1px);
    background-size: 10% 10%;
    mask-image: radial-gradient(circle, black, transparent 74%);
  }
  .radar-ticks {
    position: absolute;
    inset: 2.8%;
    border-radius: 50%;
    opacity: 0.48;
    background: repeating-conic-gradient(
      from -1deg,
      rgba(132, 239, 240, 0.48) 0deg 0.45deg,
      transparent 0.45deg 4.5deg,
      rgba(132, 239, 240, 0.25) 4.5deg 5.2deg,
      transparent 5.2deg 9deg
    );
    mask-image: radial-gradient(transparent 0 91%, black 91.4% 100%);
  }
  .radar-sector {
    position: absolute;
    z-index: 1;
    top: 50%;
    left: 50%;
    width: 42%;
    height: 1px;
    transform-origin: 0 0;
    background: linear-gradient(90deg, rgba(110, 236, 238, 0.22), transparent);
  }
  .radar-sector--one {
    transform: rotate(45deg);
  }
  .radar-sector--two {
    transform: rotate(135deg);
  }
  .radar-horizon {
    position: absolute;
    inset: 12%;
    border: 1px dashed rgba(73, 210, 220, 0.1);
    border-radius: 50%;
  }
  .radar-ring {
    position: absolute;
    inset: 24%;
    border: 1px solid rgba(73, 210, 220, 0.14);
    border-radius: 50%;
  }
  .radar-ring--two {
    inset: 39%;
  }
  .radar-ring--three {
    inset: 10%;
    border-color: rgba(89, 214, 223, 0.1);
  }
  .radar-cross {
    position: absolute;
    top: 50%;
    right: 0;
    left: 0;
    height: 1px;
    background: rgba(73, 210, 220, 0.16);
  }
  .radar-cross--y {
    transform: rotate(90deg);
  }
  .radar-sweep {
    position: absolute;
    inset: 50% 50% 0 0;
    transform-origin: 100% 0;
    background: conic-gradient(
      from 270deg at 100% 0,
      rgba(55, 230, 234, 0.34),
      rgba(55, 230, 234, 0.03) 24deg,
      transparent 48deg
    );
    animation: radar 6s linear infinite;
  }
  .radar-origin {
    position: absolute;
    z-index: 3;
    inset: 50% auto auto 50%;
    display: grid;
    width: 52px;
    height: 52px;
    place-items: center;
    border: 1px solid rgba(93, 242, 244, 0.2);
    border-radius: 50%;
    color: var(--cyan-200);
    background: rgba(4, 24, 33, 0.82);
    box-shadow: 0 0 28px rgba(40, 223, 232, 0.13);
    transform: translate(-50%, -50%);
  }
  .contact {
    position: absolute;
    z-index: 4;
    display: grid;
    grid-template-columns: auto auto;
    align-items: center;
    gap: 5px;
  }
  .contact i {
    width: 7px;
    height: 7px;
    border: 1px solid #c9ffff;
    border-radius: 50%;
    background: var(--cyan-300);
    box-shadow: 0 0 14px var(--cyan-300);
    animation: pulse 2.6s ease-in-out infinite;
  }
  .contact > span {
    display: grid;
    gap: 1px;
  }
  .contact em {
    color: var(--cyan-300);
    font-family: var(--font-display);
    font-size: 7px;
    font-style: normal;
    letter-spacing: 0.08em;
  }
  .contact small {
    color: var(--ink-400);
    font-family: var(--font-mono);
    font-size: 6px;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }
  .contact--friendly i {
    border-radius: 1px;
    color: var(--safe);
    background: var(--safe);
    box-shadow: 0 0 12px rgba(104, 215, 170, 0.45);
  }
  .contact--friendly em {
    color: var(--safe);
  }
  .contact--unknown i {
    width: 8px;
    height: 8px;
    border-radius: 0;
    color: var(--warning);
    background: transparent;
    box-shadow: none;
    transform: rotate(45deg);
  }
  .contact--unknown em {
    color: var(--amber-400);
  }
  .contact--one {
    top: 29%;
    left: 63%;
  }
  .contact--two {
    top: 66%;
    left: 31%;
  }
  .contact--three {
    top: 48%;
    left: 77%;
  }
  .contact--one {
    animation: contact-drift-one 8s ease-in-out infinite;
  }
  .contact--two {
    animation: contact-drift-two 10s ease-in-out infinite;
  }
  .contact--three {
    animation: contact-drift-three 7s ease-in-out infinite;
  }
  .fleet-trace {
    position: absolute;
    z-index: 2;
    display: flex;
    gap: 2px;
    transform: rotate(-28deg);
  }
  .fleet-trace i {
    width: 13px;
    height: 6px;
    border: 1px solid rgba(137, 230, 235, 0.35);
    background: rgba(53, 142, 157, 0.38);
  }
  .fleet-trace--one {
    top: 38%;
    left: 25%;
  }
  .fleet-trace--two {
    right: 26%;
    bottom: 30%;
    transform: rotate(36deg);
  }
  .radar-bearing,
  .radar-range,
  .radar-readout {
    position: absolute;
    z-index: 5;
    color: var(--ink-400);
    font-family: var(--font-display);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.14em;
  }
  .radar-bearing--north {
    top: 6.5%;
    left: 50%;
    color: var(--cyan-200);
    transform: translateX(-50%);
  }
  .radar-bearing--east {
    top: 50%;
    right: 7.5%;
    transform: translateY(-50%);
  }
  .radar-bearing--south {
    bottom: 6.5%;
    left: 50%;
    transform: translateX(-50%);
  }
  .radar-bearing--west {
    top: 50%;
    left: 7.5%;
    transform: translateY(-50%);
  }
  .radar-range--outer {
    top: 18%;
    right: 23%;
  }
  .radar-range--inner {
    top: 39%;
    right: 42%;
    color: rgba(163, 238, 239, 0.62);
  }
  .radar-readout {
    right: 11%;
    bottom: 11%;
    display: grid;
    gap: 2px;
    padding: 5px 7px;
    border-left: 1px solid rgba(111, 235, 235, 0.45);
    background: rgba(2, 13, 20, 0.55);
  }
  .radar-readout span {
    color: var(--ink-500);
    font-size: 6px;
  }
  .radar-readout strong {
    color: var(--cyan-200);
    font-size: 8px;
    letter-spacing: 0.08em;
  }
  .telemetry-card {
    position: absolute;
    z-index: 5;
    display: grid;
    gap: 3px;
    min-width: 144px;
    padding: 12px 14px;
    border: 1px solid var(--line);
    border-left-color: var(--cyan-300);
    border-radius: 2px 10px 10px 2px;
    background: rgba(3, 16, 24, 0.86);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.3);
    backdrop-filter: blur(14px);
  }
  .telemetry-card small,
  .visual-coordinates,
  .visual-index {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 7px;
    font-weight: 700;
    letter-spacing: 0.17em;
  }
  .telemetry-card strong {
    font-family: var(--font-display);
    font-size: 15px;
    letter-spacing: 0.08em;
  }
  .telemetry-card span {
    color: var(--ink-400);
    font-size: 7px;
  }
  .telemetry-card--top {
    top: 12%;
    right: -2%;
  }
  .telemetry-card--bottom {
    bottom: 13%;
    left: -1%;
  }
  .visual-coordinates {
    position: absolute;
    z-index: 6;
  }
  .visual-coordinates--top {
    top: 0;
    left: 50%;
    transform: translateX(-50%);
  }
  .visual-coordinates--side {
    top: 50%;
    right: -34px;
    transform: rotate(90deg);
  }
  .visual-index {
    position: absolute;
    bottom: 1%;
    left: 50%;
    display: flex;
    align-items: center;
    gap: 7px;
    transform: translateX(-50%);
  }
  .visual-index i {
    width: 56px;
    height: 1px;
    background: var(--line-strong);
  }
  .mission-brief {
    padding-block: 48px 112px;
  }
  .mission-brief__heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 40px;
    margin-bottom: 28px;
  }
  .mission-brief__heading h2 {
    margin: 0;
    font-family: var(--font-display);
    font-size: clamp(30px, 4vw, 46px);
    font-weight: 600;
  }
  .mission-brief__heading > p {
    max-width: 440px;
    margin-bottom: 4px;
    color: var(--ink-300);
    font-size: 13px;
    line-height: 1.8;
  }
  .mission-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 16px;
  }
  .mission-grid article {
    position: relative;
    min-height: 220px;
  }
  .mission-grid :global(svg) {
    color: var(--cyan-300);
  }
  .mission-grid article > small {
    display: block;
    margin: 25px 0 3px;
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.16em;
  }
  .mission-grid h3 {
    margin-bottom: 11px;
    font-size: 18px;
  }
  .mission-grid p {
    margin: 0;
    color: var(--ink-300);
    font-size: 12px;
    line-height: 1.8;
  }
  .mission-number {
    position: absolute;
    top: -8px;
    right: 0;
    color: rgba(110, 185, 201, 0.17);
    font-family: var(--font-display);
    font-size: 42px;
    font-weight: 700;
  }
  @media (max-width: 1040px) {
    .hero {
      grid-template-columns: 1fr;
      padding-top: 72px;
    }
    .hero__copy {
      max-width: 820px;
    }
    .command-visual {
      width: min(650px, 100%);
      margin-inline: auto;
    }
  }
  @media (max-width: 720px) {
    .hero {
      gap: 48px;
      min-height: auto;
      padding-block: 48px 56px;
    }
    .hero__status-line {
      display: block;
      margin-bottom: 32px;
    }
    .command-entry__controls {
      grid-template-columns: 1fr;
    }
    .command-entry__controls :global(.ui-button) {
      width: 100%;
    }
    .trust-row {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 14px;
    }
    .trust-row span:last-child {
      grid-column: 1 / -1;
    }
    .command-visual {
      width: 94%;
    }
    .telemetry-card {
      min-width: 122px;
      padding: 9px 10px;
    }
    .telemetry-card--top {
      right: -4%;
    }
    .telemetry-card--bottom {
      left: -4%;
    }
    .visual-coordinates--side {
      display: none;
    }
    .mission-brief {
      padding-block: 24px 80px;
    }
    .mission-brief__heading {
      display: block;
    }
    .mission-brief__heading > p {
      margin-top: 16px;
    }
    .mission-grid {
      grid-template-columns: 1fr;
    }
    .mission-grid article {
      min-height: auto;
    }
  }
  .hero {
    position: relative;
    max-width: var(--layout-max);
    min-height: min(900px, calc(100vh - 72px));
    overflow: clip;
  }
  .hero::before {
    position: absolute;
    z-index: -1;
    top: 22%;
    right: 0;
    bottom: 0;
    left: 0;
    content: '';
    opacity: 0.26;
    pointer-events: none;
    background: radial-gradient(ellipse at center, rgba(42, 140, 151, 0.16), transparent 65%);
  }
  .hero::after {
    position: absolute;
    z-index: -1;
    top: 50%;
    right: 7%;
    left: 30%;
    height: 1px;
    content: '';
    opacity: 0.58;
    pointer-events: none;
    background: linear-gradient(90deg, transparent, rgba(83, 233, 232, 0.28), transparent);
  }
  .hero__status-line {
    color: var(--ink-500);
    letter-spacing: 0.16em;
  }
  .display-title {
    font-family: var(--font-display);
    font-size: clamp(42px, 4.1vw, 68px);
    line-height: 1.02;
    letter-spacing: -0.025em;
  }
  .display-title__line--signal {
    margin-top: 4px;
    color: transparent;
    background: linear-gradient(105deg, #d8fbfc 4%, #75e9eb 48%, #4ea4dc 100%);
    background-clip: text;
  }
  .hero__status-line {
    margin-bottom: 28px;
  }
  :global(.command-entry) {
    border-radius: 8px 3px 8px 3px;
    border-color: rgba(83, 233, 232, 0.28);
    background: linear-gradient(145deg, rgba(8, 30, 38, 0.86), rgba(2, 13, 20, 0.94));
  }
  .command-entry__head {
    border-bottom-color: var(--line);
  }
  .command-symbol {
    border-radius: 50%;
    color: var(--tactical);
    background: rgba(83, 233, 232, 0.08);
  }
  .command-entry__head > span {
    color: var(--safe);
  }
  .command-visual {
    filter: saturate(0.84);
  }
  .radar-shell {
    border-radius: 10px 3px 10px 3px;
    clip-path: polygon(3% 0, 97% 0, 100% 3%, 100% 97%, 97% 100%, 3% 100%, 0 97%, 0 3%);
    background: radial-gradient(
      circle at 50% 48%,
      rgba(8, 78, 88, 0.32),
      rgba(2, 13, 20, 0.96) 67%
    );
  }
  .radar-shell::before {
    position: absolute;
    inset: 0;
    content: '';
    opacity: 0.22;
    background: repeating-linear-gradient(
      165deg,
      transparent 0 8px,
      rgba(93, 191, 198, 0.035) 9px 10px
    );
  }
  .telemetry-card {
    border-radius: 3px;
    border-color: var(--line);
    background: rgba(2, 13, 20, 0.86);
  }
  .mission-brief {
    padding-top: 22px;
  }
  .mission-grid {
    gap: 12px;
  }
  :global(.mission-grid .ui-surface) {
    border-radius: 7px 2px 7px 2px;
    border-color: var(--line);
    background: linear-gradient(145deg, rgba(7, 28, 36, 0.82), rgba(2, 13, 20, 0.86));
  }
  :global(.mission-grid .ui-surface:hover) {
    border-color: var(--line-active);
  }
  @media (max-width: 820px) {
    .hero {
      min-height: auto;
    }
    .display-title {
      font-size: clamp(42px, 11vw, 64px);
      line-height: 0.96;
    }
    .display-title__line {
      white-space: normal;
    }
    .hero::after {
      display: none;
    }
  }
  @media (min-width: 821px) and (max-height: 780px) {
    .hero {
      gap: clamp(34px, 5vw, 72px);
      padding-block: 32px 38px;
    }
    .hero__status-line {
      margin-bottom: 24px;
    }
    .display-title {
      font-size: clamp(40px, 3.9vw, 58px);
      line-height: 0.94;
    }
    :global(.command-entry) {
      margin-top: 20px;
    }
    .trust-row {
      margin-top: 16px;
    }
    .command-visual {
      width: min(100%, 480px);
    }
  }
  @keyframes contact-drift-one {
    50% {
      transform: translate(-7px, 5px);
    }
  }
  @keyframes contact-drift-two {
    50% {
      transform: translate(6px, -4px);
    }
  }
  @keyframes contact-drift-three {
    50% {
      transform: translate(4px, 8px);
    }
  }
</style>
