<script lang="ts">
  import { Check, Copy, LogOut, Radio, Share2, UserRound, Wifi } from '@lucide/svelte';
  import type { GameSnapshot } from '$lib/types';

  interface Props {
    snapshot: GameSnapshot;
    inviteUrl: string;
    onleave: () => void;
  }
  let { snapshot, inviteUrl, onleave }: Props = $props();
  let copied = $state(false);

  async function copyInvite() {
    try {
      await navigator.clipboard.writeText(inviteUrl);
      copied = true;
      setTimeout(() => (copied = false), 2_000);
    } catch {
      copied = false;
    }
  }

  async function shareInvite() {
    if (navigator.share) {
      await navigator.share({ title: `${snapshot.room.name} · Mk.01`, text: '온라인 해전 작전실에 참가하세요.', url: inviteUrl });
    } else await copyInvite();
  }
</script>

<section class="waiting panel" aria-labelledby="waiting-title">
  <div class="waiting__radar" aria-hidden="true"><div class="waiting__sweep"></div><Radio size={25} /></div>
  <p class="eyebrow">AWAITING SECOND COMMANDER</p>
  <h1 id="waiting-title">상대 지휘관을 기다리는 중</h1>
  <p class="muted">초대 링크나 작전 코드를 공유하십시오. 두 번째 지휘관이 합류하면 바로 함대 배치로 전환됩니다.</p>

  <div class="room-identity">
    <div><small>OPERATION</small><strong>{snapshot.room.name}</strong></div>
    <div><small>SECURE CODE</small><strong class="code">{snapshot.room.code}</strong></div>
  </div>

  <div class="invite-bar">
    <span>{inviteUrl}</span>
    <button class="icon-button" onclick={copyInvite} aria-label="초대 링크 복사" title="링크 복사">{#if copied}<Check size={16} />{:else}<Copy size={16} />{/if}</button>
    <button class="icon-button" onclick={shareInvite} aria-label="초대 링크 공유" title="공유"><Share2 size={16} /></button>
  </div>

  <div class="player-slots">
    <article class="player-slot player-slot--online"><span><UserRound size={21} /></span><div><small>HOST COMMANDER</small><strong>{snapshot.players[0]?.nickname}</strong></div><em><Wifi size={13} /> 온라인</em></article>
    <article class="player-slot player-slot--pending"><span><UserRound size={21} /></span><div><small>OPPONENT</small><strong>연결 대기 중</strong></div><em><span class="pending-dot"></span> 탐색 중</em></article>
  </div>

  <button class="button button--ghost button--small leave-button" onclick={onleave}><LogOut size={15} /> 작전실 나가기</button>
</section>

<style>
  .waiting{width:min(750px,100%);margin:0 auto;padding:46px;text-align:center}.waiting__radar{position:relative;display:grid;width:86px;height:86px;place-items:center;margin:0 auto 26px;overflow:hidden;border:1px solid rgba(57,224,235,.3);border-radius:50%;color:var(--cyan-400);background:radial-gradient(circle,rgba(33,158,178,.18),transparent 66%)}.waiting__radar::before,.waiting__radar::after{position:absolute;inset:50% 0 auto;height:1px;content:'';background:rgba(57,224,235,.15)}.waiting__radar::after{transform:rotate(90deg)}.waiting__sweep{position:absolute;inset:50% 50% 0 0;transform-origin:100% 0;background:conic-gradient(from 270deg at 100% 0,rgba(57,224,235,.4),transparent 40deg);animation:radar 2.8s linear infinite}.waiting__radar svg{position:relative;z-index:2}.waiting h1{margin-bottom:10px;font-family:Rajdhani,sans-serif;font-size:34px}.waiting>p.muted{max-width:590px;margin:0 auto;line-height:1.7}.room-identity{display:grid;grid-template-columns:1fr 1fr;margin:30px 0 14px;border:1px solid var(--line);border-radius:10px;background:rgba(3,15,24,.55)}.room-identity>div{display:grid;gap:4px;padding:15px}.room-identity>div:first-child{border-right:1px solid var(--line)}.room-identity small{color:#638091;font-family:Rajdhani;font-size:9px;letter-spacing:.16em}.room-identity strong{font-size:14px}.room-identity .code{color:var(--cyan-200);font-family:Rajdhani;font-size:22px;letter-spacing:.18em}.invite-bar{display:grid;grid-template-columns:1fr auto auto;gap:7px;align-items:center;padding:7px;border:1px solid var(--line);border-radius:10px;background:rgba(2,11,18,.74)}.invite-bar>span{overflow:hidden;padding-left:10px;color:#7894a3;text-align:left;font-size:11px;white-space:nowrap;text-overflow:ellipsis}.player-slots{display:grid;grid-template-columns:1fr 1fr;gap:10px;margin-top:25px}.player-slot{display:grid;grid-template-columns:auto 1fr auto;align-items:center;gap:10px;padding:14px;border:1px solid var(--line);border-radius:10px;text-align:left;background:rgba(8,28,40,.55)}.player-slot>span{display:grid;width:38px;height:38px;place-items:center;border:1px solid var(--line);border-radius:50%;color:#8babb9}.player-slot>div{display:grid;gap:2px}.player-slot small{color:#617e8e;font-family:Rajdhani;font-size:8px;letter-spacing:.12em}.player-slot strong{font-size:12px}.player-slot em{display:flex;align-items:center;gap:5px;color:var(--green-500);font-size:9px;font-style:normal}.player-slot--pending{border-style:dashed;opacity:.74}.player-slot--pending em{color:#7e9aa8}.pending-dot{width:5px;height:5px;border-radius:50%;background:var(--amber-500);animation:pulse 1.2s infinite}.leave-button{margin-top:25px;color:#7893a2}
  @media(max-width:650px){.waiting{padding:30px 18px}.waiting h1{font-size:28px}.room-identity,.player-slots{grid-template-columns:1fr}.room-identity>div:first-child{border-right:0;border-bottom:1px solid var(--line)}.invite-bar{grid-template-columns:minmax(0,1fr) auto auto}.player-slot{grid-template-columns:auto 1fr}.player-slot em{grid-column:2}}
</style>

