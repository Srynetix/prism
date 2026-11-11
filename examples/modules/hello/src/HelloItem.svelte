<script lang="ts">
  import { Icon, t } from '@prism/sdk'
</script>

<span class="item" title={t('Hello from a module')}><Icon name="star" />{t('Hello')}</span>

<style>
  .item { display: inline-flex; align-items: center; gap: 4px; }
</style>
