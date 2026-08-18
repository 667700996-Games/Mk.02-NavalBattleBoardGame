<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';
  import { t } from '$lib/i18n';

  interface Props {
    id: string;
    label: string;
    value?: string;
    type?: string;
    placeholder?: string;
    hint?: string;
    error?: string;
    required?: boolean;
    disabled?: boolean;
    minlength?: number;
    maxlength?: number;
    autocomplete?: HTMLInputAttributes['autocomplete'];
    code?: boolean;
  }
  let {
    id,
    label,
    value = $bindable(''),
    type = 'text',
    placeholder,
    hint,
    error,
    required = false,
    disabled = false,
    minlength,
    maxlength,
    autocomplete,
    code = false
  }: Props = $props();
</script>

<label class:error class="ui-field" for={id}>
  <span class="ui-field__label"
    >{label}{#if required}<em>{$t('common.required')}</em>{/if}</span
  >
  <span class="ui-field__control">
    <input
      {id}
      {type}
      bind:value
      {placeholder}
      {required}
      {disabled}
      {minlength}
      {maxlength}
      {autocomplete}
      class:ui-field__input--code={code}
      aria-invalid={Boolean(error)}
      aria-describedby={error ? `${id}-error` : hint ? `${id}-hint` : undefined}
    />
    <i aria-hidden="true"></i>
  </span>
  {#if error}<small id={`${id}-error`} class="ui-field__error">{error}</small>{:else if hint}<small
      id={`${id}-hint`}
      class="ui-field__hint">{hint}</small
    >{/if}
</label>
