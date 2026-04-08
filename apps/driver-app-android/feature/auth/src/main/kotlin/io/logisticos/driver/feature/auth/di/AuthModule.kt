package io.logisticos.driver.feature.auth.di

import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent

// AuthRepository uses @Singleton + @Inject constructor — no explicit binding needed.
// This module is a placeholder for future auth-specific bindings.
@Module
@InstallIn(SingletonComponent::class)
object AuthModule
