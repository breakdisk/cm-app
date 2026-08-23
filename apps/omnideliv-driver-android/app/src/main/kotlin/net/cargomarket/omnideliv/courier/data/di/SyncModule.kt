package net.cargomarket.omnideliv.courier.data.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import net.cargomarket.omnideliv.courier.data.sync.WorkManagerSyncScheduler
import net.cargomarket.omnideliv.courier.domain.SyncScheduler
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
abstract class SyncModule {

    @Binds
    @Singleton
    abstract fun syncScheduler(impl: WorkManagerSyncScheduler): SyncScheduler
}
