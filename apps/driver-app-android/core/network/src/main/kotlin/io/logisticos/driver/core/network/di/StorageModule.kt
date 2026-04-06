package io.logisticos.driver.core.network.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.logisticos.driver.core.network.auth.EncryptedTokenStorage
import io.logisticos.driver.core.network.auth.TokenStorage
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
abstract class StorageModule {
    @Binds
    @Singleton
    abstract fun bindTokenStorage(impl: EncryptedTokenStorage): TokenStorage
}
