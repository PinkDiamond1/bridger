<template>
  <v-card>
    <v-card-title>
      <v-tooltip bottom>
        <template v-slot:activator="{ on, attrs }">
          <h2 class="text-h5" v-bind="attrs" v-on="on">{{ lane }}</h2>
        </template>
        <span>Lane</span>
      </v-tooltip>
      <v-spacer/>
      <v-progress-circular v-if="loading.sourceChainOutboundLaneData" indeterminate :color="sourceChain.color"/>
      <span v-else class="subtitle-1">
        <span>[</span>
        <span>{{ source.sourceChainOutboundLaneData.latestReceivedNonce }}</span>
        <span>,</span>
        <span>{{ source.sourceChainOutboundLaneData.latestGeneratedNonce }}</span>
        <span>]</span>
      </span>
    </v-card-title>
    <v-divider/>
    <v-container>
      <v-row>
        <v-col cols="6">
          <h3 class="subtitle-1">Delivery</h3>
          <v-divider/>
          <v-simple-table dense>
            <template v-slot:default>
              <thead>
              <tr>
                <th style="width: 70%">Title</th>
                <th>Value</th>
              </tr>
              </thead>
              <tbody>
              <tr>
                <td class="subtitle-2">Oldest unpruned nonce</td>
                <td>
                  <v-progress-linear v-if="loading.sourceChainOutboundLaneData" :color="sourceChain.color"
                                     indeterminate/>
                  <span v-else v-text="source.sourceChainOutboundLaneData.oldestUnprunedNonce"/>
                </td>
              </tr>
              <tr>
                <td class="subtitle-2">Latest received nonce</td>
                <td>
                  <v-progress-linear v-if="loading.sourceChainOutboundLaneData" :color="sourceChain.color"
                                     indeterminate/>
                  <span v-else v-text="source.sourceChainOutboundLaneData.latestReceivedNonce"/>
                </td>
              </tr>
              <tr>
                <td class="subtitle-2">Latest generated nonce</td>
                <td>
                  <v-progress-linear v-if="loading.sourceChainOutboundLaneData" :color="sourceChain.color"
                                     indeterminate/>
                  <span v-else v-text="source.sourceChainOutboundLaneData.latestGeneratedNonce"/>
                </td>
              </tr>
              </tbody>
            </template>
          </v-simple-table>
        </v-col>
        <v-col cols="6">
          <h3 class="subtitle-1">Receiving</h3>
          <v-divider/>
          <v-simple-table dense>
            <template v-slot:default>
              <thead>
              <tr>
                <th style="width: 45%">Title</th>
                <th>Value</th>
              </tr>
              </thead>
              <tbody>
              <tr>
                <td class="subtitle-2">Last relayed at source</td>
                <td>
                  <v-progress-linear v-if="loading.targetChainInboundLaneData" :color="sourceChain.color"
                                     indeterminate/>
                  <external-explorer v-else :identity="source.lastTargetChainRelayedBlockAtSource" type="block"
                                    :chain="targetChain"/>
                </td>
              </tr>
              <tr>
                <td class="subtitle-2">Last dispatched</td>
                <td>
                  <v-progress-linear v-if="loading.targetChainInboundLaneData || loading.sourceChainOutboundLaneData"
                                     :color="sourceChain.color" indeterminate/>
                  <span v-else :class="{
                    'red--text': source.maxConfirmEndAtTarget < source.sourceChainOutboundLaneData.latestReceivedNonce,
                    'green--text': source.maxConfirmEndAtTarget >= source.sourceChainOutboundLaneData.latestReceivedNonce,
                  }">
                    {{ source.maxConfirmEndAtTarget }}
                  </span>
                </td>
              </tr>
              </tbody>
            </template>
          </v-simple-table>
        </v-col>
        <v-expand-transition>
          <v-col cols="12" v-if="!loading.targetChainInboundLaneData && cond.showDispatchInfo">
            <v-row>
              <v-col cols="12">
                <h3 class="subtitle-1">Dispatch info</h3>
                <v-divider class="mb-3"/>
              </v-col>
              <v-col cols="6" v-for="item in source.targetChainInboundLaneData.relayers">
                <v-card :loading="loading.targetChainInboundLaneData">
                  <v-container>
                    <v-simple-table dense>
                      <template v-slot:default>
                        <thead>
                        <tr>
                          <th style="width: 30%">Title</th>
                          <th>Value</th>
                        </tr>
                        </thead>
                        <tbody>
                        <tr>
                          <td class="subtitle-2">Relayer</td>
                          <td>
                            <external-explorer :identity="item.relayer" type="account"
                                              :chain="targetChain"/>
                          </td>
                        </tr>
                        <tr>
                          <td class="subtitle-2">Begin</td>
                          <td>{{ item.messages.begin }}</td>
                        </tr>
                        <tr>
                          <td class="subtitle-2">End</td>
                          <td>{{ item.messages.end }}</td>
                        </tr>
                        <tr>
                          <td class="subtitle-2">Result</td>
                          <td>{{ item.messages.dispatchResults }}</td>
                        </tr>
                        </tbody>
                      </template>
                    </v-simple-table>
                  </v-container>
                </v-card>
              </v-col>
            </v-row>
          </v-col>
        </v-expand-transition>
      </v-row>
    </v-container>
    <v-card-actions>
      <v-spacer/>
      <template v-if="!loading.targetChainInboundLaneData && source.targetChainInboundLaneData.relayers.length > 0">
        <v-tooltip bottom>
          <template v-slot:activator="{ on, attrs }">
            <v-btn
              icon
              v-bind="attrs"
              v-on="on"
              @click="cond.showDispatchInfo = !cond.showDispatchInfo"
            >
              <v-icon>{{ cond.showDispatchInfo ? 'mdi-chevron-up' : 'mdi-chevron-down' }}</v-icon>
            </v-btn>
          </template>
          <span>Expanded more details</span>
        </v-tooltip>
      </template>
    </v-card-actions>
  </v-card>
</template>

<script>


import ExternalExplorer from '@/components/widgets/external-explorer';

async function initState(vm) {
  const sourceChainBridgeTarget = vm.sourceChain.bridge_target[vm.targetChain.bridge_chain_name];
  vm.subscriber.sourceChainOutboundLaneData = await vm.sourceClient
    .query[sourceChainBridgeTarget.query_name.messages]
    .outboundLanes(vm.lane, async v => {
      vm.source.sourceChainOutboundLaneData = v.toJSON();
      vm.loading.sourceChainOutboundLaneData = false;
    });

  if (vm.parachainBridge) {
    await listenParachainInboundLaneData(vm);
  } else {
    await listenSolochainInboundLaneData(vm);
  }
}

async function listenSolochainInboundLaneData(vm) {
  const sourceChainBridgeTarget = vm.sourceChain.bridge_target[vm.targetChain.bridge_chain_name];
  vm.subscriber.targetRelayedBlockAtSource = await vm.sourceClient
    .query[sourceChainBridgeTarget.query_name.grandpa]
    .bestFinalized(async v => {
      vm.source.lastTargetChainRelayedBlockAtSource = v.toJSON();
      await queryTargetChainInboundLaneData(vm);
      vm.loading.targetChainInboundLaneData = false;
    });
}

async function listenParachainInboundLaneData(vm) {
  const sourceChainBridgeTarget = vm.sourceChain.bridge_target[vm.targetChain.bridge_chain_name];
  vm.subscriber.targetRelayedBlockAtSource = await vm.sourceClient
    .query[sourceChainBridgeTarget.query_name.parachains]
    .bestParaHeads(sourceChainBridgeTarget.para_id, async v => {
      // headHash
      const paraHead = v.toJSON();
      vm.source.lastTargetChainRelayedBlockAtSource = paraHead.headHash;
      await queryTargetChainInboundLaneData(vm);
      vm.loading.targetChainInboundLaneData = false;
    });
}

async function queryTargetChainInboundLaneData(vm) {
  const targetChainBridgeTarget = vm.targetChain.bridge_target[vm.sourceChain.bridge_chain_name];
  const atApi = await vm.targetClient.at(vm.source.lastTargetChainRelayedBlockAtSource);
  const inboundLaneData = await atApi
    .query[targetChainBridgeTarget.query_name.messages]
    .inboundLanes(vm.lane);
  vm.source.targetChainInboundLaneData = inboundLaneData.toJSON();
  vm.source.maxConfirmEndAtTarget = vm.$stream(vm.source.targetChainInboundLaneData.relayers)
    .map(item => item.messages.end)
    .max()
    .orElse(0);
}


export default {
  props: {
    lane: {
      type: String,
    },
    parachainBridge: {
      type: Boolean,
      default: false,
    },
    sourceClient: {
      type: Object,
    },
    targetClient: {
      type: Object,
    },
    sourceChain: {
      type: Object,
    },
    targetChain: {
      type: Object,
    },
  },
  components: {ExternalExplorer},
  data: () => ({
    source: {
      sourceChainOutboundLaneData: null,
      lastTargetChainRelayedBlockAtSource: null,
      targetChainInboundLaneData: null,
      maxConfirmEndAtTarget: null,
    },
    subscriber: {
      sourceChainOutboundLaneData: null,
      targetRelayedBlockAtSource: null,
    },
    cond: {
      showDispatchInfo: false,
    },
    loading: {
      sourceChainOutboundLaneData: true,
      targetChainInboundLaneData: true,
    },
  }),
  created() {
    initState(this);
  },
  destroyed() {
    const vm = this;
    vm.subscriber.sourceChainOutboundLaneData && vm.subscriber.sourceChainOutboundLaneData();
    vm.subscriber.targetRelayedBlockAtSource && vm.subscriber.targetRelayedBlockAtSource();
  }
}
</script>

<style scoped>

</style>
