<script lang="ts">
  import { page } from '$app/state';
  import { ArrowLeft, Radio } from '@lucide/svelte';
</script>
<svelte:head><title>{page.status === 404 ? '좌표 없음' : '시스템 오류'} · Mk.01</title></svelte:head>
<div class="error-page shell"><section class="panel"><div class="error-code">{page.status}</div><Radio size={35}/><h1>{page.status===404?'지정한 좌표를 찾을 수 없습니다':'작전 시스템 오류'}</h1><p>{page.status===404?'요청한 작전 채널이 존재하지 않거나 이동되었습니다.':'요청을 처리하지 못했습니다. 잠시 후 로비에서 다시 시도해 주세요.'}</p><a class="button button--primary" href="/lobby"><ArrowLeft size={16}/> 작전 로비로 복귀</a></section></div>
<style>.error-page{display:grid;min-height:calc(100vh - 68px);place-items:center;padding-block:40px}.error-page section{position:relative;width:min(520px,100%);padding:45px;text-align:center;overflow:hidden}.error-page svg{color:var(--cyan-400)}.error-code{position:absolute;top:-35px;right:10px;color:rgba(71,151,177,.1);font-family:Rajdhani;font-size:140px;font-weight:700}.error-page h1{position:relative;margin-top:25px;font-size:27px}.error-page p{position:relative;margin-bottom:25px;color:var(--steel-300);line-height:1.7}@media(max-width:720px){.error-page{min-height:calc(100vh - 60px)}.error-page section{padding:35px 18px}}</style>
