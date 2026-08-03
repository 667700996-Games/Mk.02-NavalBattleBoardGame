<script lang="ts">
  import { Check, Crosshair, Flame, Radio, Shield, Waves, X } from '@lucide/svelte';
  import GridBoard from './GridBoard.svelte';
  import { sounds } from '$lib/sound';
  import { FLEET, coordinateKey, coordinateLabel, shipName, type Coordinate, type GameSnapshot } from '$lib/types';

  interface Props {
    snapshot: GameSnapshot;
    pending?: boolean;
    disabled?: boolean;
    onfire: (coordinate: Coordinate) => void;
  }
  let { snapshot, pending = false, disabled = false, onfire }: Props = $props();
  let selected = $state<Coordinate | null>(null);
  let activeBoard = $state<'target' | 'own'>('target');

  let myTurn = $derived(snapshot.currentPlayerId === snapshot.selfPlayerId);
  let me = $derived(snapshot.players.find((player) => player.id === snapshot.selfPlayerId));
  let opponent = $derived(snapshot.players.find((player) => player.id !== snapshot.selfPlayerId));
  let attackedKeys = $derived(new Set(snapshot.targetBoard?.attacks.map((attack) => coordinateKey(attack.coordinate)) ?? []));
  let canFire = $derived(Boolean(selected && myTurn && !pending && !disabled && !attackedKeys.has(coordinateKey(selected))));
  let sunkShips = $derived(new Set(snapshot.targetBoard?.attacks.filter((attack) => attack.sunkShip).map((attack) => attack.sunkShip) ?? []));

  function choose(coordinate: Coordinate) {
    if (!myTurn || pending || disabled || attackedKeys.has(coordinateKey(coordinate))) return;
    selected = coordinate;
    sounds.select();
  }

  function fire() {
    if (!selected || !canFire) return;
    onfire(selected);
    selected = null;
  }
</script>

<section class="battle" aria-labelledby="battle-status">
  <header class:turn-banner--mine={myTurn} class="turn-banner panel">
    <div class="turn-banner__icon">{#if myTurn}<Crosshair size={24} />{:else}<Radio size={24} />{/if}</div>
    <div><span>TURN {String(snapshot.turnNumber ?? 0).padStart(2,'0')}</span><h1 id="battle-status">{disabled ? '통신 복구 대기' : myTurn ? '공격 좌표를 지정하십시오' : `${opponent?.nickname ?? '상대'} 지휘관의 응답 대기`}</h1></div>
    <div class="turn-banner__side"><small>CURRENT COMMAND</small><strong class:cyan={myTurn}>{myTurn ? 'YOUR TURN' : 'OPPONENT'}</strong></div>
  </header>

  <div class="mobile-tabs" role="tablist" aria-label="전투 보드 선택">
    <button class:active={activeBoard === 'target'} role="tab" aria-selected={activeBoard === 'target'} onclick={() => (activeBoard = 'target')}><Crosshair size={15} /> 공격 해역</button>
    <button class:active={activeBoard === 'own'} role="tab" aria-selected={activeBoard === 'own'} onclick={() => (activeBoard = 'own')}><Shield size={15} /> 아군 해역</button>
  </div>

  <div class="battle-grid">
    <div class:hidden-mobile={activeBoard !== 'target'} class="board-panel panel">
      <div class="board-panel__heading"><div><span>ENEMY WATERS</span><h2>상대 공격 보드</h2></div><em>{snapshot.targetBoard?.attacks.length ?? 0}회 공격</em></div>
      <GridBoard mode="target" label="상대 해역 공격 보드" targetBoard={snapshot.targetBoard} {selected} interactive={myTurn} {disabled} oncell={choose} />
      <div class="board-legend"><span><i class="legend-miss"></i> 빗나감</span><span><i class="legend-hit"></i> 명중</span><span><i class="legend-sunk"></i> 격침</span></div>
    </div>

    <div class:hidden-mobile={activeBoard !== 'own'} class="board-panel panel">
      <div class="board-panel__heading"><div><span>FRIENDLY WATERS</span><h2>아군 함선 보드</h2></div><em>{snapshot.ownBoard?.attacksReceived.length ?? 0}회 피격</em></div>
      <GridBoard mode="own" label="아군 함선 방어 보드" ownBoard={snapshot.ownBoard} disabled={true} />
      <div class="fleet-health">
        {#each snapshot.ownBoard?.ships ?? [] as ship}
          <span class:sunk={ship.sunk} title={`${shipName(ship.kind)} ${ship.hits.length}/${ship.cells.length}`}><i style={`--health:${(ship.cells.length-ship.hits.length)/ship.cells.length}`}></i></span>
        {/each}
      </div>
    </div>

    <aside class="fire-control panel">
      <div class="fire-control__title"><Crosshair size={17} /><div><small>FIRE CONTROL</small><strong>사격 통제</strong></div></div>
      <div class:coordinate-lock--active={selected} class="coordinate-lock"><small>SELECTED COORDINATE</small><strong>{selected ? coordinateLabel(selected) : '— —'}</strong><span>{selected ? '좌표 잠금 완료' : '공격 보드에서 좌표 선택'}</span></div>
      <button class="button button--primary button--wide fire-button" disabled={!canFire} onclick={fire}>{#if pending}<span class="mini-spinner"></span> 판정 대기{:else}<Crosshair size={17} /> 공격 실행{/if}</button>
      {#if selected}<button class="clear-selection" onclick={() => (selected = null)}><X size={13} /> 선택 취소</button>{/if}
      <div class="enemy-fleet"><small>ENEMY FLEET STATUS</small>{#each FLEET as ship}<div class:sunk={sunkShips.has(ship.kind)}><span>{ship.name}</span><span class="mini-ship">{#each Array.from({length:ship.size}) as _}<i></i>{/each}</span>{#if sunkShips.has(ship.kind)}<Check size={13} />{/if}</div>{/each}</div>
      <div class="commanders"><div><span class="online-dot"></span><small>YOU</small><strong>{me?.nickname}</strong></div><div><span class:offline-dot={opponent?.connectionState !== 'ONLINE'} class="online-dot"></span><small>OPPONENT</small><strong>{opponent?.nickname}</strong></div></div>
    </aside>
  </div>
</section>

<style>
  .turn-banner{display:grid;grid-template-columns:auto 1fr auto;align-items:center;gap:16px;margin-bottom:18px;padding:16px 20px;border-radius:14px}.turn-banner--mine{border-color:rgba(57,224,235,.44);background:linear-gradient(100deg,rgba(14,65,80,.96),rgba(5,25,37,.96));box-shadow:0 12px 50px rgba(22,199,217,.08)}.turn-banner__icon{display:grid;width:45px;height:45px;place-items:center;border:1px solid var(--line-strong);border-radius:50%;color:var(--cyan-400);background:rgba(22,199,217,.08)}.turn-banner span,.turn-banner__side small{color:#6c8999;font-family:Rajdhani;font-size:9px;letter-spacing:.15em}.turn-banner h1{margin:3px 0 0;font-size:17px}.turn-banner__side{display:grid;gap:3px;text-align:right}.turn-banner__side strong{font-family:Rajdhani;font-size:13px;letter-spacing:.11em}.battle-grid{display:grid;grid-template-columns:minmax(0,1fr) minmax(0,1fr) 265px;gap:16px;align-items:start}.board-panel{padding:14px;border-radius:15px}.board-panel__heading{display:flex;align-items:end;justify-content:space-between;margin:2px 4px 12px}.board-panel__heading span,.fire-control small{color:#617e8e;font-family:Rajdhani;font-size:8px;letter-spacing:.15em}.board-panel__heading h2{margin:3px 0 0;font-size:14px}.board-panel__heading em{color:#7794a4;font-size:9px;font-style:normal}.board-legend{display:flex;justify-content:center;gap:15px;margin-top:11px;color:#7895a5;font-size:9px}.board-legend span{display:flex;align-items:center;gap:5px}.board-legend i{width:7px;height:7px;border-radius:50%}.legend-miss{background:#6bb6d1}.legend-hit{background:#ff7e46;box-shadow:0 0 5px #ff6a3d}.legend-sunk{background:#ff5364}.fleet-health{display:grid;grid-template-columns:repeat(5,1fr);gap:5px;margin-top:10px}.fleet-health span{height:5px;overflow:hidden;border-radius:5px;background:#1c3645}.fleet-health i{display:block;width:calc(var(--health)*100%);height:100%;background:var(--green-500)}.fleet-health span.sunk i{background:var(--red-500)}.fire-control{padding:17px;border-radius:15px}.fire-control__title{display:flex;align-items:center;gap:10px;padding-bottom:14px;border-bottom:1px solid var(--line);color:var(--cyan-400)}.fire-control__title div{display:grid;gap:2px}.fire-control__title strong{color:#d9e9f0;font-size:13px}.coordinate-lock{display:grid;place-items:center;min-height:125px;margin:14px 0;padding:14px;border:1px dashed rgba(87,154,179,.23);border-radius:10px;background:rgba(2,13,21,.58);text-align:center}.coordinate-lock--active{border-color:rgba(255,180,60,.52);background:rgba(96,61,13,.1)}.coordinate-lock strong{margin:6px 0 3px;color:#668494;font-family:Rajdhani;font-size:32px;letter-spacing:.18em}.coordinate-lock--active strong{color:var(--amber-500);text-shadow:0 0 18px rgba(255,180,60,.25)}.coordinate-lock span{color:#607d8d;font-size:9px}.fire-button{min-height:48px}.clear-selection{display:flex;align-items:center;justify-content:center;gap:4px;width:100%;margin-top:7px;border:0;color:#6f8b9a;background:none;cursor:pointer;font-size:9px}.mini-spinner{width:14px;height:14px;border:2px solid rgba(0,20,24,.25);border-top-color:#04161a;border-radius:50%;animation:spin .7s linear infinite}.enemy-fleet{display:grid;gap:7px;margin-top:20px;padding-top:16px;border-top:1px solid var(--line)}.enemy-fleet>small{margin-bottom:3px}.enemy-fleet>div{display:grid;grid-template-columns:1fr auto 14px;align-items:center;gap:6px;color:#a7bdc8;font-size:10px}.enemy-fleet>div.sunk{color:#607b8a;text-decoration:line-through}.enemy-fleet>div.sunk svg{color:var(--red-500)}.mini-ship{display:flex;gap:1px}.mini-ship i{width:5px;height:4px;background:#4f8295}.sunk .mini-ship i{background:#6a3743}.commanders{display:grid;grid-template-columns:1fr 1fr;gap:6px;margin-top:18px;padding-top:15px;border-top:1px solid var(--line)}.commanders>div{position:relative;display:grid;gap:2px;padding-left:10px}.commanders small{font-size:7px}.commanders strong{overflow:hidden;font-size:9px;white-space:nowrap;text-overflow:ellipsis}.online-dot{position:absolute;top:4px;left:0;width:5px;height:5px;border-radius:50%;background:var(--green-500);box-shadow:0 0 6px var(--green-500)}.offline-dot{background:var(--red-500);box-shadow:0 0 6px var(--red-500)}.mobile-tabs{display:none}
  @media(max-width:1120px){.battle-grid{grid-template-columns:1fr 1fr}.fire-control{grid-column:1/-1;display:grid;grid-template-columns:180px minmax(200px,1fr) 220px;gap:15px;align-items:center}.fire-control__title{border:0;padding:0}.coordinate-lock{min-height:90px;margin:0}.enemy-fleet{grid-column:1/-1;grid-template-columns:repeat(5,1fr);margin:0}.enemy-fleet>small{grid-column:1/-1}.commanders{display:none}.clear-selection{display:none}}
  @media(max-width:720px){.turn-banner{grid-template-columns:auto 1fr;padding:13px}.turn-banner__side{display:none}.turn-banner h1{font-size:14px}.mobile-tabs{display:grid;grid-template-columns:1fr 1fr;margin-bottom:10px;padding:3px;border:1px solid var(--line);border-radius:10px;background:rgba(4,16,25,.7)}.mobile-tabs button{display:flex;min-height:40px;align-items:center;justify-content:center;gap:7px;border:0;border-radius:7px;color:#7794a4;background:transparent;font-size:11px}.mobile-tabs button.active{color:var(--cyan-200);background:rgba(31,117,141,.28)}.battle-grid{display:block}.board-panel{padding:8px}.board-panel.hidden-mobile{display:none}.fire-control{position:sticky;z-index:20;bottom:max(8px,env(safe-area-inset-bottom));display:grid;grid-template-columns:1fr auto;gap:8px;margin-top:10px;padding:10px;background:rgba(6,23,34,.97);backdrop-filter:blur(14px)}.fire-control__title,.enemy-fleet,.commanders{display:none}.coordinate-lock{display:flex;min-height:48px;align-items:center;justify-content:space-between;margin:0;padding:7px 11px;text-align:left}.coordinate-lock small,.coordinate-lock span{display:none}.coordinate-lock strong{margin:0;font-size:25px}.fire-button{width:auto;min-width:135px}.clear-selection{display:none}.board-legend{margin-bottom:3px}}
</style>

