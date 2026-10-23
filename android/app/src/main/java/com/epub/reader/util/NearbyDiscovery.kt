package com.zhongbai233.epub.reader.util

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Handler
import android.os.Looper
import com.zhongbai233.epub.reader.model.DiscoveredPeer

/** Native DNS-SD; advertisements are only hints, never pairing credentials. */
class NearbyDiscovery(context:Context,private val changed:(List<DiscoveredPeer>)->Unit){
    private val manager=context.getSystemService(Context.NSD_SERVICE) as NsdManager
    private val main=Handler(Looper.getMainLooper())
    private var browser:NsdManager.DiscoveryListener?=null
    private var registration:NsdManager.RegistrationListener?=null
    private val peers=mutableMapOf<String,DiscoveredPeer>()
    private var epoch=0
    private var ownId=""
    private val present=mutableSetOf<String>()
    private val queue=java.util.ArrayDeque<NsdServiceInfo>()
    private var resolving=false
    fun start(){if(browser!=null)return;val generation=++epoch
        val listener=object:NsdManager.DiscoveryListener{
            override fun onDiscoveryStarted(type:String){}
            override fun onDiscoveryStopped(type:String){}
            override fun onStartDiscoveryFailed(type:String,code:Int){main.post{if(epoch==generation){browser=null;peers.clear();changed(emptyList())}}}
            override fun onStopDiscoveryFailed(type:String,code:Int){}
            override fun onServiceLost(info:NsdServiceInfo){main.post{if(epoch==generation){present.remove(info.serviceName);peers.remove(info.serviceName);changed(peers.values.toList())}}}
            override fun onServiceFound(info:NsdServiceInfo){
                if(!info.serviceType.startsWith("_epubreader._tcp"))return
                main.post{if(epoch==generation && present.add(info.serviceName)){queue.add(info);resolveNext(generation)}}
            }
        };browser=listener;runCatching{manager.discoverServices("_epubreader._tcp.",NsdManager.PROTOCOL_DNS_SD,listener)}.onFailure{browser=null}
    }
    @Suppress("DEPRECATION") private fun resolveNext(generation:Int){
        if(epoch!=generation || resolving)return
        var info=queue.poll()?:return
        while(!present.contains(info.serviceName)){info=queue.poll()?:return}
        resolving=true
        val listener=object:NsdManager.ResolveListener{
            private fun complete(service:NsdServiceInfo?){main.post{
                if(epoch!=generation)return@post
                resolving=false
                if(service!=null && present.contains(service.serviceName)){
                    val id=service.attributes["id"]?.toString(Charsets.UTF_8).orEmpty()
                    val host=service.host?.hostAddress.orEmpty()
                    if(id!=ownId && runCatching{java.util.UUID.fromString(id)}.isSuccess && host.isNotEmpty() && !host.contains(':')){
                        val name=service.attributes["name"]?.toString(Charsets.UTF_8)?.take(80)?:service.serviceName
                        peers[service.serviceName]=DiscoveredPeer(id,name,"$host:${service.port}",System.currentTimeMillis()/1000)
                        changed(peers.values.toList())
                    }
                }
                resolveNext(generation)
            }}
            override fun onResolveFailed(service:NsdServiceInfo,code:Int){complete(null)}
            override fun onServiceResolved(service:NsdServiceInfo){complete(service)}
        }
        runCatching{manager.resolveService(info,listener)}.onFailure{resolving=false;resolveNext(generation)}
    }
    fun publish(id:String,name:String,port:Int){ownId=id;peers.entries.removeAll{it.value.deviceId==id};changed(peers.values.toList());unpublish();if(port<=0||id.isEmpty())return
        val listener=object:NsdManager.RegistrationListener{
            override fun onServiceRegistered(info:NsdServiceInfo){}
            override fun onRegistrationFailed(info:NsdServiceInfo,code:Int){}
            override fun onServiceUnregistered(info:NsdServiceInfo){}
            override fun onUnregistrationFailed(info:NsdServiceInfo,code:Int){}
        };registration=listener
        val info=NsdServiceInfo().apply{serviceName="reader-${id.take(24)}";serviceType="_epubreader._tcp.";this.port=port;setAttribute("id",id);setAttribute("name",name.take(80))}
        runCatching{manager.registerService(info,NsdManager.PROTOCOL_DNS_SD,listener)}.onFailure{registration=null}
    }
    fun unpublish(){registration?.let{runCatching{manager.unregisterService(it)}};registration=null}
    fun close(){++epoch;browser?.let{runCatching{manager.stopServiceDiscovery(it)}};browser=null;peers.clear();present.clear();queue.clear();resolving=false;unpublish()}
}
