<script lang="ts">
  import { Hand, Keyboard, MousePointer2 } from '@lucide/svelte';
  import { inputModality, type InputModality } from '$lib/stores';

  interface Props {
    context: 'placement' | 'targeting' | 'chat';
    compact?: boolean;
  }

  let { context, compact = false }: Props = $props();

  const labels: Record<InputModality, string> = {
    pointer: '마우스',
    keyboard: '키보드',
    touch: '터치'
  };
  const prompts: Record<Props['context'], Record<InputModality, string>> = {
    placement: {
      pointer: '함선 선택 → 해역 클릭 · R 회전 · 자동 배치',
      keyboard: 'Tab 함선 선택 · 방향키 좌표 이동 · Space 배치 · R 회전 · Esc 해제',
      touch: '함선 탭 → 해역 탭 · 회전 버튼 · 자동 배치'
    },
    targeting: {
      pointer: '공격 보드 클릭 → 공격 실행',
      keyboard: 'Tab 보드 진입 · 방향키 이동 · Space 좌표 선택 · 공격 실행',
      touch: '공격 좌표 탭 → 공격 실행 탭'
    },
    chat: {
      pointer: '메시지 입력 · Enter 전송 · 아이콘으로 신호 선택',
      keyboard: 'Enter 전송 · Shift+Enter 줄바꿈 · Escape 닫기',
      touch: '메시지 입력 → 전송 버튼 · 아이콘으로 빠른 신호'
    }
  };

  let label = $derived(labels[$inputModality]);
  let prompt = $derived(prompts[context][$inputModality]);
</script>

<aside
  class:input-prompt--compact={compact}
  class="input-prompt"
  aria-label={`${label} 입력 도움말`}
  aria-live="polite"
  data-testid={`input-prompt-${context}`}
  data-modality={$inputModality}
>
  <span aria-hidden="true">
    {#if $inputModality === 'keyboard'}
      <Keyboard size={14} />
    {:else if $inputModality === 'touch'}
      <Hand size={14} />
    {:else}
      <MousePointer2 size={14} />
    {/if}
  </span>
  <strong>{label}</strong>
  <em>{prompt}</em>
</aside>

<style>
  .input-prompt {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 7px;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 7px;
    color: var(--ink-300);
    background: rgba(3, 17, 25, 0.62);
  }
  .input-prompt > span {
    display: grid;
    flex: none;
    color: var(--cyan-300);
  }
  .input-prompt strong {
    flex: none;
    color: var(--cyan-200);
    font: 700 9px var(--font-display);
    letter-spacing: 0.08em;
  }
  .input-prompt em {
    min-width: 0;
    overflow: hidden;
    font-size: 10px;
    font-style: normal;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .input-prompt--compact {
    padding: 0;
    border: 0;
    background: transparent;
  }
  @media (max-width: 720px) {
    .input-prompt {
      align-items: flex-start;
    }
    .input-prompt em {
      line-height: 1.45;
      white-space: normal;
    }
  }
</style>
